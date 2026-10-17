"""Archive safety regressions for the real Linux portable boot acceptance runner."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("portable", Path(__file__).with_name("gpui-linux-portable-smoke.py"))
portable = importlib.util.module_from_spec(spec)
spec.loader.exec_module(portable)

FILES = {"industrialis", "LICENSE", "THIRD_PARTY_NOTICES.txt", "README.txt", "icon.png", "industrialis-gpui.desktop"}


class ArchiveAcceptance(unittest.TestCase):
    def archive(self, root, extra=None):
        archive = root / "portable.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            directory = tarfile.TarInfo("industrialis-gpui")
            directory.type = tarfile.DIRTYPE
            tar.addfile(directory)
            for name in sorted(FILES):
                entry = tarfile.TarInfo("industrialis-gpui/" + name)
                data = (name + " bytes").encode()
                entry.size = len(data)
                entry.mode = 0o755 if name == "industrialis" else 0o644
                tar.addfile(entry, io.BytesIO(data))
            if extra is not None:
                tar.addfile(extra)
        return archive

    def test_expected_files_extract_with_executable_permissions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = portable.unpack_archive(self.archive(root), root / "output")
            self.assertEqual({item.name for item in folder.iterdir()}, FILES)
            self.assertEqual((folder / "industrialis").read_bytes(), b"industrialis bytes")
            self.assertEqual((folder / "industrialis").stat().st_mode & 0o777, 0o755)

    def test_rejects_links_duplicate_files_and_unexpected_paths_before_extraction(self):
        for kind in ("symlink", "hardlink", "traversal", "duplicate", "directory"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                entry = tarfile.TarInfo({"duplicate": "industrialis-gpui/LICENSE",
                                        "traversal": "../escaped", "directory": "industrialis-gpui/extra"}.get(kind, "industrialis-gpui/link"))
                if kind in ("symlink", "hardlink"):
                    entry.type = tarfile.SYMTYPE if kind == "symlink" else tarfile.LNKTYPE
                    entry.linkname = "../../escaped"
                elif kind == "directory":
                    entry.type = tarfile.DIRTYPE
                with self.assertRaises(AssertionError):
                    portable.unpack_archive(self.archive(root, entry), root / "output")
                self.assertFalse((root / "output").exists())
                self.assertFalse((root / "escaped").exists())


if __name__ == "__main__":
    unittest.main()
