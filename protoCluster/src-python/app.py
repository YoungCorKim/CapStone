from load_file import Load_File
from save_file import Save_File
from vault_manager import Vault_Manager

load = Vault_Manager()

file = load.load_directory(r"C:\Users\chuon\OneDrive\Documents\Obsidian Vault")

if "error" in file:
    print(file["error"])
else:
    print(file["success"])

vault = load.extract_file_name()

for file in vault:
    print(file)

save = Save_File()

result = save.delete_file(r"C:\Users\chuon\OneDrive\Documents\Obsidian Vault\Pumping Lemma.md")

print(result)



