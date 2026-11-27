import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

const DEFAULT_BACKEND_URL = 'http://127.0.0.1:8080';

function normalizeBackendUrl(rawValue: string | undefined): string {
  const provided = rawValue?.trim();
  const candidate = provided ? provided : DEFAULT_BACKEND_URL;

  let parsed: URL;
  try {
    parsed = new URL(candidate);
  } catch {
    throw new Error(
      `BACKEND_URL must be an absolute http(s) origin without /api; received "${candidate}".`,
    );
  }

  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(
      `BACKEND_URL must use http or https; received "${candidate}".`,
    );
  }

  if (parsed.search || parsed.hash) {
    throw new Error(
      `BACKEND_URL must not include a query string or fragment; received "${candidate}".`,
    );
  }

  const pathname = parsed.pathname.replace(/\/+$/, '');
  const originPath = pathname.endsWith('/api')
    ? pathname.slice(0, -'/api'.length)
    : pathname;
  const normalized = `${parsed.origin}${originPath}`.replace(/\/+$/, '');

  return normalized || parsed.origin;
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), 'BACKEND_URL');
  const backendUrl = normalizeBackendUrl(process.env.BACKEND_URL ?? env.BACKEND_URL);
  const proxy = {
    '/api': {
      target: backendUrl,
      changeOrigin: true,
      secure: false,
    },
  };

  return {
    plugins: [react()],
    server: {
      host: 'localhost',
      port: 5173,
      strictPort: true,
      proxy,
    },
    preview: {
      host: 'localhost',
      port: 5173,
      strictPort: true,
      proxy,
    },
  };
});
