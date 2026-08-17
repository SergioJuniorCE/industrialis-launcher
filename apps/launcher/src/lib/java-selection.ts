type JavaPathTester = (path: string) => Promise<unknown>;

type JavaSelectionHandler = (path: string) => void;

type JavaSelectionErrorHandler = (message: string) => void;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function validateAndSelectJava(
  selectedPath: string | null | undefined,
  testJava: JavaPathTester,
  onSelect: JavaSelectionHandler,
  onError: JavaSelectionErrorHandler,
): Promise<void> {
  const path = selectedPath?.trim();
  if (!path) return;

  try {
    await testJava(path);
    onSelect(path);
  } catch (error) {
    onError(`Selected file is not a usable Java runtime: ${errorMessage(error)}`);
  }
}
