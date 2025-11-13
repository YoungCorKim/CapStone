import os
import json

class Load_File:
    def __init__(self):
        pass
    

    def load_file(self, file_path: str):
        if not os.path.exists(file_path):
            return {"error": f"File not found: {file_path}"}
        
        if not file_path.endswith(".md"):
            return {"error": "Only markdown (.md) files are supported."}
        
        try:
            with open(file_path, "r", encoding="utf-8") as file:
                content = file.read()
            return {
                "name": os.path.basename(file_path),
                "path": os.path.abspath(file_path),
                "content" : content
            }
        except Exception as e:
            return {"error": str(e)}
        
    def load_directory(self, path: str):
        if not os.path.exists(path):
            return {"error": f"Directory does not exist: {path}"}

        new_vault = {}
        try:
            for root, dirs, files in os.walk(path):
                for file in files:
                    if file.endswith(".md"):
                        full_path = os.path.join(root, file)
                        loaded_file = self.load_file(full_path)
                        if "error" in loaded_file:
                            continue
                        new_vault[full_path] = loaded_file

        except Exception as e:
            return {"error": f"Failed to load directory: {path} ({str(e)})"}

        return new_vault

