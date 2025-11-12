import os
import json

class Load_File:
    def _inti_(self):
        self.__vault = {}
        pass

    def _load_file(self, file_path: str):
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
                "content": content
            }
        except Exception as e:
            return {"error": str(e)}

    def load_directory(self, path: str):
        new_vault = {}  
        try:
            for root, dirs, files in os.walk(path):
                for file in files:
                    if file.endswith(".md"):
                        full_path = os.path.join(root, file)
                        loaded_file = self._load_file(full_path)
                        if "error" in file: 
                            continue
                        new_vault[loaded_file["name"]] = loaded_file
        except Exception as e:
            return {"error": f"Fail to load directory: {path} ({str(e)})"}
        
        self.vault = new_vault
        return{"successful": "File loaded successful"}

    def get_vault(self):
        return self.vault

