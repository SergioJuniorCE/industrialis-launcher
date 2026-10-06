export const DOCKER_ENGINE_DOCS_PATH = "/docs#docker-engine-unavailable";

export function isDockerEngineError(message: string): boolean {
  return message.includes("Docker Engine is unavailable");
}
