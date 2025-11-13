from load_file import Load_File
from save_file import Save_File
from vault_manager import Vault_Manager
from rule_manager import Rule_Manager
from deduplicate import Deduplicate

load = Vault_Manager()

load.load_directory(r"C:\Users\chuon\OneDrive\Documents\First Fault")

vault = load.get_vault()

print(vault.keys())

dedu = Deduplicate("gpt-3.5-turbo")

result = dedu.deduplicate(vault)

for key, content in result.items():
    print(key)
    print(content)
    print()


