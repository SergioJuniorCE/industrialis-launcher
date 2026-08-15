export interface JavaInfo {
  path: string;
  version: string;
  majorVersion: number;
  architecture: string;
  vendor: string;
}

export type JavaSortKey = "version" | "architecture" | "path";
export type JavaSortDirection = "asc" | "desc";

const naturalCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function sameJavaPath(left: string | null | undefined, right: string | null | undefined): boolean {
  if (!left || !right) return false;
  const windowsPaths = /^[a-z]:[\\/]/iu.test(left) && /^[a-z]:[\\/]/iu.test(right);
  return windowsPaths ? left.toLocaleLowerCase() === right.toLocaleLowerCase() : left === right;
}

export function filterJavaInstallations(installations: JavaInfo[], query: string): JavaInfo[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return installations;
  return installations.filter((java) =>
    [java.version, String(java.majorVersion), java.architecture, java.vendor, java.path].some((value) => value.toLocaleLowerCase().includes(normalized)),
  );
}

export function sortJavaInstallations(installations: JavaInfo[], key: JavaSortKey, direction: JavaSortDirection): JavaInfo[] {
  const multiplier = direction === "asc" ? 1 : -1;
  return [...installations].sort((left, right) => {
    let comparison: number;
    if (key === "version") {
      comparison = left.majorVersion - right.majorVersion || naturalCollator.compare(left.version, right.version);
    } else {
      comparison = naturalCollator.compare(left[key], right[key]);
    }
    return (comparison === 0 ? naturalCollator.compare(left.path, right.path) : comparison) * multiplier;
  });
}
