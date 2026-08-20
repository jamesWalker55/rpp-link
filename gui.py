# /// script
# dependencies = [
#   "tkinterdnd2",
# ]
# ///

"""
Simple UI script written by claude
"""

import subprocess
import tkinter as tk
from tkinter import font as tkfont

try:
    from tkinterdnd2 import DND_FILES, TkinterDnD
except ImportError:
    raise SystemExit(
        "Missing dependency 'tkinterdnd2'. Install it with:\n"
        "    pip install tkinterdnd2"
    )

COMMAND = "rpp-link.exe"


def decode_bytes(data: bytes) -> str:
    """
    Decode subprocess output robustly. rpp-link.exe's output isn't
    guaranteed to be in the system's default codepage (e.g. cp1252 on
    Windows), so try utf-8 first, then fall back to cp1252, then finally
    to latin-1 with replacement characters so we never crash.
    """
    if not data:
        return ""
    for encoding in ("utf-8", "cp1252"):
        try:
            return data.decode(encoding)
        except UnicodeDecodeError:
            continue
    return data.decode("latin-1", errors="replace")


def parse_drop_data(data: str) -> str:
    """
    tkinterdnd2 gives file paths wrapped in {curly braces} if they contain
    spaces, and multiple files separated by spaces. We just take the first
    file dropped.
    """
    data = data.strip()
    if data.startswith("{") and "}" in data:
        return data[1 : data.index("}")]
    # No braces: could still be multiple space-separated paths; take first.
    return data.split()[0] if data else ""


class App:
    def __init__(self, root: TkinterDnD.Tk):
        self.root = root
        self.root.title("rpp-link drag & drop")
        self.root.geometry("800x500")

        mono = tkfont.Font(family="Consolas", size=11)
        if mono.actual("family").lower() != "consolas":
            mono = tkfont.Font(family="Courier New", size=11)

        self.status = tk.Label(
            root,
            text="Drop a file here to run: rpp-link.exe <path>",
            anchor="w",
            padx=8,
            pady=4,
        )
        self.status.pack(fill="x", side="top")

        frame = tk.Frame(root)
        frame.pack(fill="both", expand=True)

        scrollbar = tk.Scrollbar(frame)
        scrollbar.pack(side="right", fill="y")

        self.text = tk.Text(
            frame,
            wrap="none",
            font=mono,
            yscrollcommand=scrollbar.set,
            undo=False,
        )
        self.text.pack(side="left", fill="both", expand=True)
        scrollbar.config(command=self.text.yview)

        # Read-only but still selectable/copyable: block edits, allow
        # selection and Ctrl+C.
        self.text.bind("<Key>", self._block_edit_keys)
        self.text.insert("1.0", "Drop a file onto this window to begin.")

        # Enable drag-and-drop onto the whole window and the text box.
        for widget in (self.root, self.text, frame, self.status):
            widget.drop_target_register(DND_FILES)
            widget.dnd_bind("<<Drop>>", self.on_drop)

    def _block_edit_keys(self, event):
        # Allow navigation/selection/copy shortcuts; block anything that
        # would modify the text.
        allowed_keysyms = {
            "Left",
            "Right",
            "Up",
            "Down",
            "Home",
            "End",
            "Prior",
            "Next",
            "Shift_L",
            "Shift_R",
            "Control_L",
            "Control_R",
        }
        if event.keysym in allowed_keysyms:
            return None
        if (event.state & 0x4) and event.keysym.lower() in ("c", "a"):
            # Ctrl+C / Ctrl+A
            return None
        return "break"

    def on_drop(self, event):
        path = parse_drop_data(event.data)
        if not path:
            return
        self.run_command(path)

    def run_command(self, path: str):
        self.status.config(text=f"Running: {COMMAND} {path}")
        self.root.update_idletasks()

        try:
            result = subprocess.run(
                [COMMAND, path],
                capture_output=True,
                text=False,  # get raw bytes; decode ourselves below
            )
            stdout = decode_bytes(result.stdout)
            stderr = decode_bytes(result.stderr)
            output = stdout
            if stderr:
                if output and not output.endswith("\n"):
                    output += "\n"
                output += stderr
            if not output.strip():
                output = f"(no output, exit code {result.returncode})"
            self.status.config(
                text=f"{COMMAND} {path}  (exit code {result.returncode})"
            )
        except FileNotFoundError:
            output = f"Error: could not find '{COMMAND}'. Is it on your PATH?"
            self.status.config(text=f"Failed to run {COMMAND}")
        except Exception as exc:
            output = f"Error running command:\n{exc}"
            self.status.config(text=f"Failed to run {COMMAND}")

        self.text.delete("1.0", "end")
        self.text.insert("1.0", output)


def main():
    root = TkinterDnD.Tk()
    App(root)
    root.mainloop()


if __name__ == "__main__":
    main()
