import os
from pathlib import Path
import shutil

class Save_File:
    def __init__(self):
        pass

    def save_file(self, file_path: str, content: str, backup: bool = False):
        path = Path(file_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists() and backup:
            backup_path = path.with_suffix(path.suffix + ".bak")
            shutil.copy(path, backup_path)
            return {"saved": "File saved successfully"}
        
        try:
            with open(path, "w", encoding="utf-8") as file:
                file.write(content)
            return {"create": "New file created successfully"}
        except Exception as e:
            return {"error":f"Error saving file {path}: {e}"}

    def delete_file(self, file_path: str):
        path = Path(file_path)
        if not path.exists():
            return {"error": f"File does not exist: {file_path}"}
        
        try:
            path.unlink()
            return {"success": file_path}
        except Exception as e:
            return {"error": f"Failed to delete file: {file_path} ({str(e)})"}
