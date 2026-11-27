import { spawnSync } from 'node:child_process';
import type { E2EEnvironment } from './env';

const DOCKER_BINARY = process.env.DOCKER_BIN?.trim() || 'docker';
const DEFAULT_TIMEOUT_MS = 60_000;
const MAX_BUFFER_BYTES = 32 * 1024 * 1024;

export interface DockerRunOptions {
  timeoutMs?: number;
}

function displayCommand(host: string | null, args: string[]): string {
  return [DOCKER_BINARY, ...(host ? ['--host', host] : []), ...args].join(' ');
}

export function runDocker(
  host: string | null,
  args: string[],
  options: DockerRunOptions = {},
): string {
  const env = { ...process.env };
  if (host) {
    delete env.DOCKER_HOST;
  }

  const command = host ? ['--host', host, ...args] : args;
  const result = spawnSync(DOCKER_BINARY, command, {
    encoding: 'utf8',
    env,
    maxBuffer: MAX_BUFFER_BYTES,
    timeout: options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  });

  if (result.error) {
    throw new Error(
      `docker ${displayCommand(host, args)} failed: ${result.error.message}`,
    );
  }
  if (result.signal) {
    throw new Error(
      `docker ${displayCommand(host, args)} was terminated by ${result.signal}`,
    );
  }
  if (result.status !== 0) {
    const stderr = (result.stderr || '').trim();
    const stdout = (result.stdout || '').trim();
    throw new Error(
      `docker ${displayCommand(host, args)} exited ${result.status}: ` +
        `${stderr || stdout || 'no output'}`,
    );
  }
  return (result.stdout || '').trim();
}

export function innerDocker(
  env: E2EEnvironment,
  args: string[],
  options?: DockerRunOptions,
): string {
  return runDocker(env.innerDockerHost, args, options);
}

export function outerDocker(
  env: E2EEnvironment,
  args: string[],
  options?: DockerRunOptions,
): string {
  return runDocker(
    env.outerDockerHost ?? process.env.DOCKER_HOST?.trim() ?? null,
    args,
    options,
  );
}

export function innerDockerJson<T>(
  env: E2EEnvironment,
  args: string[],
  options?: DockerRunOptions,
): T {
  const output = innerDocker(env, args, options);
  try {
    return JSON.parse(output) as T;
  } catch (error) {
    throw new Error(
      `docker ${args.join(' ')} returned invalid JSON: ${
        error instanceof Error ? error.message : String(error)
      }; output=${output.slice(0, 2000)}`,
    );
  }
}

export function dockerLines(
  env: E2EEnvironment,
  args: string[],
  options?: DockerRunOptions,
): string[] {
  const output = innerDocker(env, args, options);
  return output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
