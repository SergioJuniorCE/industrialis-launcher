type JavaPathTester = (path: string) => Promise<unknown>;

type JavaSelectionHandler = (path: string) => void;

type JavaSelectionErrorHandler = (message: string) => void;

type JavaSelectionCurrentCheck = () => boolean;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function validateAndSelectJava(
  selectedPath: string | null | undefined,
  testJava: JavaPathTester,
  onSelect: JavaSelectionHandler,
  onError: JavaSelectionErrorHandler,
  isCurrent: JavaSelectionCurrentCheck = () => true,
): Promise<void> {
  const path = selectedPath?.trim();
  if (!path || !isCurrent()) return;

  try {
    await testJava(path);
    if (isCurrent()) onSelect(path);
  } catch (error) {
    if (isCurrent()) onError(`Selected file is not a usable Java runtime: ${errorMessage(error)}`);
  }
}
