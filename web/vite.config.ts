import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

const server = process.env.COMETTRACE_SERVER ?? 'http://localhost:4318'
const api = { target: server, changeOrigin: false }

/* The reset mail links to /auth/reset?token=…, which is a screen of this app and not a server route,
   so that one path has to stay with the dev server while the rest of /auth is proxied. */
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': path.resolve(__dirname, 'src') } },
  server: {
    proxy: {
      '^/auth/(?!reset($|\\?))': api,
      '^/health$': api,
      '^/v1/': api,
    },
  },
})
