"""Archive safety regressions for the real Linux portable boot acceptance runner."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("portable", Path(__file__).with_name("linux-portable-smoke.py"))
portable = importlib.util.module_from_spec(spec)
spec.loader.exec_module(portable)

FILES = {"industrialis", "LICENSE", "THIRD_PARTY_NOTICES.txt", "README.txt", "icon.png", "industrialis-launcher.desktop"}


class ArchiveAcceptance(unittest.TestCase):
    def archive(self, root, extra=None):
        archive = root / "portable.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            directory = tarfile.TarInfo("industrialis-launcher")
            directory.type = tarfile.DIRTYPE
            tar.addfile(directory)
            for name in sorted(FILES):
                entry = tarfile.TarInfo("industrialis-launcher/" + name)
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
                entry = tarfile.TarInfo({"duplicate": "industrialis-launcher/LICENSE",
                                        "traversal": "../escaped", "directory": "industrialis-launcher/extra"}.get(kind, "industrialis-launcher/link"))
                if kind in ("symlink", "hardlink"):
                    entry.type = tarfile.SYMTYPE if kind == "symlink" else tarfile.LNKTYPE
                    entry.linkname = "../../escaped"
                elif kind == "directory":
                    entry.type = tarfile.DIRTYPE
                with self.assertRaises(AssertionError):
                    portable.unpack_archive(self.archive(root, entry), root / "output")
                self.assertFalse((root / "output").exists())
                self.assertFalse((root / "escaped").exists())


class RenderAcceptance(unittest.TestCase):
    def xlib(self, pixel):
        xlib = mock.Mock()
        xlib.XOpenDisplay.return_value = 1
        xlib.XGetImage.return_value = 2
        xlib.XGetPixel.side_effect = pixel
        return xlib

    def test_sparse_rendered_pixels_are_counted_and_saved(self):
        # A small rendered label can fall entirely between the old ten-pixel
        # sample points, particularly now that the empty details pane is gone.
        xlib = self.xlib(lambda image, x, y: x if y == 17 and 17 <= x <= 25 else 0)
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(portable.c, "CDLL", return_value=xlib):
            image = Path(temporary) / "render.png"
            self.assertEqual(portable.capture(3, 45, 45, image), 10)
            self.assertTrue(image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"))

    def test_waits_for_a_presented_frame(self):
        child = mock.Mock()
        child.poll.return_value = None
        with mock.patch.object(portable, "capture", side_effect=[1, 10]) as capture, mock.patch.object(portable.time, "sleep"):
            self.assertEqual(portable.wait_for_render(child, 3, 45, 45, Path("render.png")), 10)
            self.assertEqual(capture.call_count, 2)

    def test_blank_frame_is_rejected_and_retained_as_evidence(self):
        child = mock.Mock()
        child.poll.return_value = None
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(portable.c, "CDLL", return_value=self.xlib(lambda *args: 0)):
            image = Path(temporary) / "blank.png"
            with self.assertRaisesRegex(AssertionError, "1 distinct colors"):
                portable.wait_for_render(child, 3, 45, 45, image, timeout=0)
            self.assertTrue(image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"))


if __name__ == "__main__":
    unittest.main()
