from load_file import Load_File

load = Load_File()

file = load.load_directory(r"C:\Users\chuon\OneDrive\Documents\First Fault")

if "error" in file:
    print(file["error"])
else:
    print(file["successful"])

vault = load.get_vault()

for name, data in vault.items():
    print(data["name"])
    print(data["path"])
    print(data["content"])
    print()


