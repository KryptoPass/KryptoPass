"""The pre-push gate must reject every kind of uncommitted Git change."""

from contextlib import contextmanager
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


CHECKER = Path(__file__).resolve().parents[1] / "tools" / "check_clean.py"


class CleanTreeTests(unittest.TestCase):
    def git(self, repository, *arguments):
        return subprocess.run(
            ["git", *arguments],
            cwd=repository,
            capture_output=True,
            text=True,
            check=True,
        )

    @contextmanager
    def repository(self):
        with tempfile.TemporaryDirectory() as directory:
            repository = Path(directory)
            self.git(repository, "init", "--quiet")
            self.git(repository, "config", "user.name", "Clean Tree Tests")
            self.git(repository, "config", "user.email", "tests@example.invalid")
            self.git(repository, "config", "commit.gpgsign", "false")
            self.git(repository, "config", "core.hooksPath", str(repository / "hooks"))
            (repository / "tracked.txt").write_text("committed\n", encoding="utf-8")
            self.git(repository, "add", "tracked.txt")
            self.git(repository, "commit", "--quiet", "-m", "Create test fixture")
            yield repository

    def check(self, repository):
        return subprocess.run(
            [sys.executable, str(CHECKER)],
            cwd=repository,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_accepts_clean_repository(self):
        with self.repository() as repository:
            result = self.check(repository)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_rejects_uncommitted_changes(self):
        states = ("untracked", "unstaged", "staged", "deleted", "staged-new")
        for state in states:
            with self.subTest(state=state), self.repository() as repository:
                tracked = repository / "tracked.txt"
                if state in ("unstaged", "staged"):
                    tracked.write_text("changed\n", encoding="utf-8")
                    if state == "staged":
                        self.git(repository, "add", "tracked.txt")
                elif state == "deleted":
                    tracked.unlink()
                else:
                    (repository / "new.txt").write_text("new\n", encoding="utf-8")
                    if state == "staged-new":
                        self.git(repository, "add", "new.txt")

                result = self.check(repository)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
