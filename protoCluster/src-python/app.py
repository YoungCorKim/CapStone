from load_file import Load_File
from save_file import Save_File
from vault_manager import Vault_Manager
from rule_manager import Rule_Manager

rule = Rule_Manager()

result = rule.load_rule()

print(result)

#print(rule.get_rules_by_stage())


