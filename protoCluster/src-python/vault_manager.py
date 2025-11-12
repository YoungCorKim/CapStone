from load_file import Load_File
from save_file import Save_File
from pathlib import Path

class Vault_Manager:
    def __init__(self):
        self._vault = {}
        self._file_loader = Load_File()
        self._file_saver = Save_File()

    """
    Load the entire directory from a given file path.
    Return the status
    """
    def load_directory(self, path: str):
        result = self._file_loader.load_directory(path)
        if "error" in result:
            return {"error": f"Fail to load directory: {path}"}

        self._vault = result
        return {"success": "File loaded successful"}


    """
    Load content of a file from a given path
    """
    def load_file(self, file_path: str):
        result = self._file_loader.load_file(file_path)
        if "error" in result:
            return {"error": f"File does not exist: {file_path}"}
        
        return {"success": result}


    """
    Extract all file names from the current directory
    """
    def extract_file_name(self):
        file_paths = set()

        for file in self._vault.values():
            file_paths.add(file["path"])

        return file_paths
    
    def delete_file(self, file_path: str):
        result = self._file_saver.delete_file(file_path)

        if "success" in result:
            file_name = Path(file_path).stem
            if file_name in self._vault:
                self._vault.pop(file_name)
            return {"success": f"File deleted successfully: {file_name}"}
        
        return result

    def save_file(self, file_path: str, content: str, backup: bool = False):
        result = self._file_saver.save_file(file_path, content, backup)

        if "error" in result:
            return result
        
        new_file = self.load_file(file_path)
        self._vault[file_path] = new_file
                
        return result