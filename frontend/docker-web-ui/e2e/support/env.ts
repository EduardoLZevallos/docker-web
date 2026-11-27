export interface E2EEnvironment {
  backendUrl: string;
  innerDockerHost: string;
  outerDockerHost: string | null;
  runId: string;
  runLabel: string;
  dindId: string;
  backendId: string;
}

function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(
      `${name} is required. Run this suite through the managed frontend E2E runner; ` +
        'it starts the backend, private DinD daemon, and Vite preview.',
    );
  }
  return value;
}

function backendOrigin(): string {
  const raw =
    process.env.BACKEND_URL?.trim() ||
    process.env.E2E_BACKEND_URL?.trim() ||
    '';
  if (!raw) {
    throw new Error(
      'BACKEND_URL is required for browser E2E; pass the exact backend origin without /api.',
    );
  }

  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new Error(`BACKEND_URL must be an absolute http(s) origin; received "${raw}".`);
  }
  if (
    (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') ||
    parsed.pathname !== '/' ||
    parsed.search ||
    parsed.hash
  ) {
    throw new Error(
      `BACKEND_URL must be an http(s) origin without path, query, or fragment; received "${raw}".`,
    );
  }
  return parsed.origin;
}

function dockerHost(raw: string): string {
  const value = raw.trim();
  if (!value) {
    throw new Error('E2E_DOCKER_SOCKET must not be empty.');
  }
  if (value.startsWith('unix://') || value.startsWith('tcp://')) {
    return value;
  }
  if (!value.startsWith('/')) {
    throw new Error(
      `E2E_DOCKER_SOCKET must be an absolute socket path or Docker host; received "${value}".`,
    );
  }
  return `unix://${value}`;
}

export function e2eEnvironment(): E2EEnvironment {
  const socket = required('E2E_DOCKER_SOCKET');
  const outer = process.env.E2E_OUTER_DOCKER_HOST?.trim();
  return {
    backendUrl: backendOrigin(),
    innerDockerHost: dockerHost(socket),
    outerDockerHost: outer || null,
    runId: required('E2E_RUN_ID'),
    runLabel: required('E2E_RUN_LABEL'),
    dindId: required('E2E_DIND_ID'),
    backendId: required('E2E_BACKEND_ID'),
  };
}
