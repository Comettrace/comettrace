use comettrace_core::{
    AccountId, Connecting, Connections, Created, Destination, Filings, ProjectId, ProjectName,
    Projects, Provider, ProviderToken, TenantId,
};
use comettrace_storage::{
    CONTROL_PLANE, PostgresConnections, PostgresFilings, PostgresMemberships, PostgresProjects,
    SealedVault,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row};
use uuid::Uuid;

const A_CREDENTIAL: &str = r#"{"access_token":"lin_oauth_nothing","expires_at_unix_seconds":null}"#;

async fn control_plane(schema: &str) -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connecting to postgres");
    sqlx::query(AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&pool)
    .await
    .expect("dropping a leftover schema");
    sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .expect("creating the test schema");
    pool.close().await;

    let owned = schema.to_owned();
    let scoped = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let schema = owned.clone();
            Box::pin(async move {
                sqlx::query(AssertSqlSafe(format!("SET search_path TO {schema}")))
                    .execute(connection)
                    .await
                    .map(|_| ())
            })
        })
        .connect(&url)
        .await
        .expect("connecting to the test schema");
    CONTROL_PLANE
        .run(&scoped)
        .await
        .expect("the migrations apply");
    Some(scoped)
}

async fn an_account(pool: &PgPool, email: &str) -> AccountId {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'chosen')")
        .bind(id)
        .bind(email)
        .execute(pool)
        .await
        .expect("inserting an account");
    AccountId::new(id)
}

async fn an_org(pool: &PgPool, name: &str, owner: &str) -> (TenantId, AccountId) {
    let account = an_account(pool, owner).await;
    let org = PostgresMemberships::new(pool.clone())
        .found(name, owner)
        .await
        .expect("the organisation is founded");
    (org, account)
}

async fn a_project(pool: &PgPool, org: TenantId, name: &str) -> ProjectId {
    let projects = PostgresProjects::new(pool.clone());
    assert_eq!(
        projects
            .create(org, &ProjectName::try_from(name).expect("an admitted name"))
            .await
            .expect("a create"),
        Created::Done
    );
    projects
        .of(org)
        .await
        .expect("the projects are read")
        .into_iter()
        .find(|project| &*project.name == name)
        .unwrap_or_else(|| panic!("{name} is in the catalogue"))
        .id
}

/* Every test here needs a connection to aim, because a destination with nothing connected is a
row the foreign key will not let exist. */
async fn connected(pool: &PgPool, org: TenantId, by: AccountId, provider: Provider) {
    let credential = ProviderToken::new(A_CREDENTIAL);
    PostgresConnections::new(
        pool.clone(),
        SealedVault::from_hex_key(&SealedVault::fresh_key()).expect("a fresh key is a good one"),
    )
    .connect(
        org,
        Connecting {
            provider,
            account_label: "Acme Engineering",
            credential: &credential,
            by,
        },
    )
    .await
    .expect("the connecting");
}

fn somewhere(id: &str, name: &str) -> Destination {
    Destination {
        id: id.into(),
        name: name.into(),
    }
}

#[tokio::test]
async fn an_organisation_with_a_connection_and_no_choice_has_no_default() {
    let Some(pool) = control_plane("filings_nothing_chosen").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;

    let routings = PostgresFilings::new(pool.clone())
        .of(org)
        .await
        .expect("the routings");

    assert_eq!(routings.len(), 1);
    assert_eq!(routings[0].provider, Provider::Linear);
    assert_eq!(routings[0].default, None);
    assert!(routings[0].projects.is_empty());
}

#[tokio::test]
async fn a_default_and_one_project_that_differs_read_back_apart() {
    let Some(pool) = control_plane("filings_default_and_override").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;
    let api = a_project(&pool, org, "api").await;
    let filings = PostgresFilings::new(pool.clone());

    filings
        .aim(
            org,
            Provider::Linear,
            None,
            Some(&somewhere("team-1", "Engineering (ENG)")),
        )
        .await
        .expect("the default is set");
    filings
        .aim(
            org,
            Provider::Linear,
            Some(web),
            Some(&somewhere("team-2", "Billing (BIL)")),
        )
        .await
        .expect("the project is aimed");

    let routings = filings.of(org).await.expect("the routings");
    assert_eq!(
        routings[0].default,
        Some(somewhere("team-1", "Engineering (ENG)"))
    );
    assert_eq!(
        routings[0].projects,
        vec![(web, somewhere("team-2", "Billing (BIL)"))]
    );
    /* The project that chose nothing is absent rather than listed against the default, which is
    what lets a screen say "follows the organisation" about it. */
    assert!(!routings[0].projects.iter().any(|(id, _)| *id == api));
}

#[tokio::test]
async fn aiming_a_project_twice_moves_it_rather_than_adding_a_second_choice() {
    let Some(pool) = control_plane("filings_aim_again").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;
    let filings = PostgresFilings::new(pool.clone());

    for aimed in [
        somewhere("team-1", "Engineering (ENG)"),
        somewhere("team-2", "Billing (BIL)"),
    ] {
        filings
            .aim(org, Provider::Linear, Some(web), Some(&aimed))
            .await
            .expect("the project is aimed");
    }

    let routings = filings.of(org).await.expect("the routings");
    assert_eq!(
        routings[0].projects,
        vec![(web, somewhere("team-2", "Billing (BIL)"))]
    );
}

#[tokio::test]
async fn clearing_a_project_sends_it_back_to_the_default() {
    let Some(pool) = control_plane("filings_clear_a_project").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;
    let filings = PostgresFilings::new(pool.clone());
    filings
        .aim(
            org,
            Provider::Linear,
            None,
            Some(&somewhere("team-1", "Engineering (ENG)")),
        )
        .await
        .expect("the default is set");
    filings
        .aim(
            org,
            Provider::Linear,
            Some(web),
            Some(&somewhere("team-2", "Billing (BIL)")),
        )
        .await
        .expect("the project is aimed");

    filings
        .aim(org, Provider::Linear, Some(web), None)
        .await
        .expect("the project is cleared");

    let routings = filings.of(org).await.expect("the routings");
    assert!(routings[0].projects.is_empty());
    assert_eq!(
        routings[0].default,
        Some(somewhere("team-1", "Engineering (ENG)"))
    );
}

#[tokio::test]
async fn clearing_the_default_leaves_the_projects_that_chose_for_themselves() {
    let Some(pool) = control_plane("filings_clear_the_default").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;
    let filings = PostgresFilings::new(pool.clone());
    filings
        .aim(
            org,
            Provider::Linear,
            None,
            Some(&somewhere("team-1", "Engineering (ENG)")),
        )
        .await
        .expect("the default is set");
    filings
        .aim(
            org,
            Provider::Linear,
            Some(web),
            Some(&somewhere("team-2", "Billing (BIL)")),
        )
        .await
        .expect("the project is aimed");

    filings
        .aim(org, Provider::Linear, None, None)
        .await
        .expect("the default is cleared");

    let routings = filings.of(org).await.expect("the routings");
    assert_eq!(routings[0].default, None);
    assert_eq!(
        routings[0].projects,
        vec![(web, somewhere("team-2", "Billing (BIL)"))]
    );
}

/* The point of the composite foreign key: a project of one organisation cannot be aimed by
another, whatever the request says, because there is no row for that pair to point at. */
#[tokio::test]
async fn another_organisations_project_cannot_be_aimed() {
    let Some(pool) = control_plane("filings_across_organisations").await else {
        return;
    };
    let (mine, by) = an_org(&pool, "Acme", "owner@example.com").await;
    let (theirs, _) = an_org(&pool, "Globex", "other@example.com").await;
    connected(&pool, mine, by, Provider::Linear).await;
    let theirs_project = a_project(&pool, theirs, "web").await;

    let refused = PostgresFilings::new(pool.clone())
        .aim(
            mine,
            Provider::Linear,
            Some(theirs_project),
            Some(&somewhere("team-1", "Engineering (ENG)")),
        )
        .await
        .expect_err("a refusal");

    assert_eq!(refused, comettrace_core::ConnectionError::NotConnected);
}

#[tokio::test]
async fn a_provider_nobody_is_connected_to_cannot_be_aimed() {
    let Some(pool) = control_plane("filings_unconnected_provider").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;

    let refused = PostgresFilings::new(pool.clone())
        .aim(
            org,
            Provider::Github,
            Some(web),
            Some(&somewhere("acme/web", "acme/web")),
        )
        .await
        .expect_err("a refusal");

    assert_eq!(refused, comettrace_core::ConnectionError::NotConnected);
}

/* The other half of the composite key. A team in a workspace nobody is connected to any more is
not somewhere to file, so disconnecting takes every project's choice with it rather than leaving
rows that would come back wrong on reconnecting to a different workspace. */
#[tokio::test]
async fn disconnecting_a_provider_takes_every_choice_with_it() {
    let Some(pool) = control_plane("filings_disconnecting").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;
    let filings = PostgresFilings::new(pool.clone());
    filings
        .aim(
            org,
            Provider::Linear,
            Some(web),
            Some(&somewhere("team-2", "Billing (BIL)")),
        )
        .await
        .expect("the project is aimed");

    let dropped = PostgresConnections::new(
        pool.clone(),
        SealedVault::from_hex_key(&SealedVault::fresh_key()).expect("a fresh key is a good one"),
    )
    .disconnect(org, Provider::Linear)
    .await
    .expect("the disconnecting");

    assert!(dropped);
    assert!(filings.of(org).await.expect("the routings").is_empty());
    assert_eq!(left(&pool, org).await, 0);
}

/* Deleting a project should take its choice with it too, and this is the half of that the
cascade is responsible for. */
#[tokio::test]
async fn deleting_a_project_takes_its_choice_with_it() {
    let Some(pool) = control_plane("filings_deleting_a_project").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    connected(&pool, org, by, Provider::Linear).await;
    let web = a_project(&pool, org, "web").await;
    let filings = PostgresFilings::new(pool.clone());
    filings
        .aim(
            org,
            Provider::Linear,
            Some(web),
            Some(&somewhere("team-2", "Billing (BIL)")),
        )
        .await
        .expect("the project is aimed");

    sqlx::query("DELETE FROM projects WHERE org_id = $1 AND id = $2")
        .bind(org.as_uuid())
        .bind(web.as_uuid())
        .execute(&pool)
        .await
        .expect("deleting the project");

    assert_eq!(left(&pool, org).await, 0);
    assert!(
        filings
            .of(org)
            .await
            .expect("the routings")
            .first()
            .expect("the connection is still there")
            .projects
            .is_empty()
    );
}

/* The order the providers come back in is the order the enum declares, which is the order the
screen reads them in. It is pinned here because the query has to name the table to get it: a bare
`kind` in the order by would sort the text cast and hand back an alphabet. */
#[tokio::test]
async fn the_providers_come_back_in_the_order_the_enum_declares() {
    let Some(pool) = control_plane("filings_provider_order").await else {
        return;
    };
    let (org, by) = an_org(&pool, "Acme", "owner@example.com").await;
    for provider in [Provider::Jira, Provider::Linear, Provider::Github] {
        connected(&pool, org, by, provider).await;
    }

    let routings = PostgresFilings::new(pool.clone())
        .of(org)
        .await
        .expect("the routings");

    assert_eq!(
        routings
            .iter()
            .map(|routing| routing.provider)
            .collect::<Vec<_>>(),
        vec![Provider::Linear, Provider::Github, Provider::Jira]
    );
}

async fn left(pool: &PgPool, org: TenantId) -> i64 {
    sqlx::query("SELECT count(*) AS how_many FROM issue_destinations WHERE org_id = $1")
        .bind(org.as_uuid())
        .fetch_one(pool)
        .await
        .expect("counting the rows")
        .get("how_many")
}
