use comettrace_core::{
    AccountId, Connecting, ConnectionError, Connections, NoVault, Provider, ProviderToken, TenantId,
};
use comettrace_storage::{CONTROL_PLANE, PostgresConnections, PostgresMemberships, SealedVault};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool, Row};
use uuid::Uuid;

/* A Jira credential is the widest of the three — a token, a refresh token and an expiry — so it
is what the round trip is tested with: whatever shape a provider hands back, it is one sealed
value here. */
const A_CREDENTIAL: &str =
    r#"{"access":"atlassian-access","refresh":"atlassian-refresh","expires_at":1700003600}"#;

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

async fn an_org(pool: &PgPool, owner: &str) -> (TenantId, AccountId) {
    let account = an_account(pool, owner).await;
    let org = PostgresMemberships::new(pool.clone())
        .found("Acme", owner)
        .await
        .expect("the organisation is founded");
    (org, account)
}

fn a_vault() -> SealedVault {
    SealedVault::from_hex_key(&SealedVault::fresh_key()).expect("a fresh key is a good one")
}

fn connecting<'a>(
    provider: Provider,
    label: &'a str,
    credential: &'a ProviderToken,
    by: AccountId,
) -> Connecting<'a> {
    Connecting {
        provider,
        account_label: label,
        credential,
        by,
    }
}

async fn stored(pool: &PgPool, org: TenantId) -> Vec<(Vec<u8>, Vec<u8>)> {
    sqlx::query("SELECT nonce AS nonce, secret AS secret FROM integrations WHERE org_id = $1")
        .bind(org.as_uuid())
        .fetch_all(pool)
        .await
        .expect("reading the rows")
        .into_iter()
        .map(|row| (row.get("nonce"), row.get("secret")))
        .collect()
}

#[tokio::test]
async fn a_connection_reads_back_without_its_secret_and_opens_with_it() {
    let Some(pool) = control_plane("connections_round_trip").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());
    let credential = ProviderToken::new(A_CREDENTIAL);

    store
        .connect(
            org,
            connecting(Provider::Jira, "acme.atlassian.net", &credential, by),
        )
        .await
        .expect("the connecting");

    let listed = store.of(org).await.expect("the connections");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].provider, Provider::Jira);
    assert_eq!(&*listed[0].account_label, "acme.atlassian.net");
    assert_eq!(listed[0].connected_by, Some(by));
    assert!(listed[0].connected_at_unix_nanos > 1_600_000_000_000_000_000);
    assert_eq!(
        store
            .credential(org, Provider::Jira)
            .await
            .expect("the opening")
            .expect("a credential")
            .exposed(),
        A_CREDENTIAL
    );
}

/* The property the whole table exists for. A column dump must not carry the credential, and the
plaintext must appear nowhere in the bytes of the row. */
#[tokio::test]
async fn the_credential_is_nowhere_in_the_row_in_the_clear() {
    let Some(pool) = control_plane("connections_sealed").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());

    store
        .connect(
            org,
            connecting(
                Provider::Linear,
                "Acme",
                &ProviderToken::new(A_CREDENTIAL),
                by,
            ),
        )
        .await
        .expect("the connecting");

    for (nonce, secret) in stored(&pool, org).await {
        let bytes = [nonce, secret].concat();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("atlassian-access"),
            "the plaintext reached the row"
        );
    }
}

#[tokio::test]
async fn connecting_again_replaces_the_row_and_draws_a_fresh_nonce() {
    let Some(pool) = control_plane("connections_again").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());
    let first = ProviderToken::new(A_CREDENTIAL);
    store
        .connect(org, connecting(Provider::Linear, "Acme", &first, by))
        .await
        .expect("the first connecting");
    let before = stored(&pool, org).await;

    let second = ProviderToken::new("a-second-credential");
    store
        .connect(org, connecting(Provider::Linear, "Acme Two", &second, by))
        .await
        .expect("the second connecting");

    let after = stored(&pool, org).await;
    assert_eq!(after.len(), 1, "reconnecting replaces rather than adds");
    assert_ne!(before[0].0, after[0].0, "and draws a fresh nonce");
    assert_eq!(
        store
            .credential(org, Provider::Linear)
            .await
            .expect("the opening")
            .expect("a credential")
            .exposed(),
        "a-second-credential"
    );
    assert_eq!(
        &*store.of(org).await.expect("the connections")[0].account_label,
        "Acme Two"
    );
}

#[tokio::test]
async fn the_three_providers_are_three_connections_of_one_organisation() {
    let Some(pool) = control_plane("connections_three").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());
    let credential = ProviderToken::new(A_CREDENTIAL);
    for provider in Provider::every() {
        store
            .connect(
                org,
                connecting(provider, provider.as_str(), &credential, by),
            )
            .await
            .expect("the connecting");
    }

    let listed = store.of(org).await.expect("the connections");

    assert_eq!(
        listed.iter().map(|one| one.provider).collect::<Vec<_>>(),
        vec![Provider::Linear, Provider::Github, Provider::Jira],
        "in the order the enum declares them, which is the order the screen reads"
    );
}

#[tokio::test]
async fn the_connections_of_another_organisation_are_not_read() {
    let Some(pool) = control_plane("connections_theirs").await else {
        return;
    };
    let (mine, by) = an_org(&pool, "mine@example.com").await;
    let (theirs, _) = an_org(&pool, "theirs@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());
    store
        .connect(
            mine,
            connecting(
                Provider::Linear,
                "Mine",
                &ProviderToken::new(A_CREDENTIAL),
                by,
            ),
        )
        .await
        .expect("the connecting");

    assert!(store.of(theirs).await.expect("the connections").is_empty());
    assert!(
        store
            .credential(theirs, Provider::Linear)
            .await
            .expect("the opening")
            .is_none()
    );
}

#[tokio::test]
async fn a_provider_nothing_is_connected_to_has_no_credential() {
    let Some(pool) = control_plane("connections_absent").await else {
        return;
    };
    let (org, _) = an_org(&pool, "owner@example.com").await;

    let found = PostgresConnections::new(pool, a_vault())
        .credential(org, Provider::Github)
        .await
        .expect("the control plane answers");

    assert!(found.is_none(), "absent, not unopenable");
}

#[tokio::test]
async fn disconnecting_drops_it_once_and_then_there_is_nothing_to_drop() {
    let Some(pool) = control_plane("connections_drop").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());
    store
        .connect(
            org,
            connecting(
                Provider::Github,
                "acme/storefront",
                &ProviderToken::new(A_CREDENTIAL),
                by,
            ),
        )
        .await
        .expect("the connecting");

    let first = store
        .disconnect(org, Provider::Github)
        .await
        .expect("the dropping");
    let again = store
        .disconnect(org, Provider::Github)
        .await
        .expect("the second dropping");

    assert!(first);
    assert!(!again);
    assert!(store.of(org).await.expect("the connections").is_empty());
}

/* The key changed, or the row came from another deployment. It must be told apart from a
provider that is simply not connected, because the useful thing to say is different: connect it
again, rather than connect it. */
#[tokio::test]
async fn a_row_sealed_under_another_key_says_so_rather_than_reading_as_absent() {
    let Some(pool) = control_plane("connections_rekeyed").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    PostgresConnections::new(pool.clone(), a_vault())
        .connect(
            org,
            connecting(
                Provider::Linear,
                "Acme",
                &ProviderToken::new(A_CREDENTIAL),
                by,
            ),
        )
        .await
        .expect("the connecting");

    let refused = PostgresConnections::new(pool.clone(), a_vault())
        .credential(org, Provider::Linear)
        .await;

    assert_eq!(refused.err(), Some(ConnectionError::Unopenable));
    assert_eq!(
        PostgresConnections::new(pool, a_vault())
            .of(org)
            .await
            .expect("the connections")
            .len(),
        1,
        "and it still lists as connected, because it is"
    );
}

/* A deployment that set no key reads what is connected and cannot connect anything. The refusal
comes before the statement, so nothing is written. */
#[tokio::test]
async fn a_deployment_with_no_key_connects_nothing_and_writes_nothing() {
    let Some(pool) = control_plane("connections_unkeyed").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    let store = PostgresConnections::new(pool.clone(), NoVault);

    let refused = store
        .connect(
            org,
            connecting(
                Provider::Linear,
                "Acme",
                &ProviderToken::new(A_CREDENTIAL),
                by,
            ),
        )
        .await;

    assert_eq!(refused.err(), Some(ConnectionError::NoKey));
    assert!(stored(&pool, org).await.is_empty());
    assert!(
        store.of(org).await.expect("the connections").is_empty(),
        "and reading is still something it can do"
    );
}

#[tokio::test]
async fn deleting_an_organisation_takes_its_connections_with_it() {
    let Some(pool) = control_plane("connections_cascade").await else {
        return;
    };
    let (org, by) = an_org(&pool, "owner@example.com").await;
    PostgresConnections::new(pool.clone(), a_vault())
        .connect(
            org,
            connecting(
                Provider::Linear,
                "Acme",
                &ProviderToken::new(A_CREDENTIAL),
                by,
            ),
        )
        .await
        .expect("the connecting");

    sqlx::query("DELETE FROM orgs WHERE id = $1")
        .bind(org.as_uuid())
        .execute(&pool)
        .await
        .expect("deleting the organisation");

    let left: i64 = sqlx::query("SELECT count(*) AS value FROM integrations")
        .fetch_one(&pool)
        .await
        .expect("counting")
        .get("value");
    assert_eq!(left, 0);
}

/* Losing the account that connected it must not lose the connection: the credential is the
organisation's, not that person's. */
#[tokio::test]
async fn deleting_the_account_that_connected_it_leaves_the_connection_standing() {
    let Some(pool) = control_plane("connections_connector").await else {
        return;
    };
    let (org, _) = an_org(&pool, "owner@example.com").await;
    let leaver = an_account(&pool, "leaver@example.com").await;
    let store = PostgresConnections::new(pool.clone(), a_vault());
    store
        .connect(
            org,
            connecting(
                Provider::Linear,
                "Acme",
                &ProviderToken::new(A_CREDENTIAL),
                leaver,
            ),
        )
        .await
        .expect("the connecting");

    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(leaver.as_uuid())
        .execute(&pool)
        .await
        .expect("deleting the account");

    let listed = store.of(org).await.expect("the connections");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].connected_by, None, "nobody, rather than nothing");
    assert!(
        store
            .credential(org, Provider::Linear)
            .await
            .expect("the opening")
            .is_some(),
        "and it still works"
    );
}
