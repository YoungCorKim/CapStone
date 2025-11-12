import os
import glob
import yaml
from load_file import Load_File


class Rule_Manager:
    def __init__(self, rules_root = "rules"):
        self._rules_root = rules_root
        self._rules = []

    def load_rule(self):
        pipeline = ["pipeline.md", "deduplicate.md", "cluster.md", "enrich", "summarize.md"]
        rules_root = os.path.join(os.path.dirname(__file__), "..", self._rules_root)
        file_loader = Load_File()
        for stage in pipeline:
            rule_file_path = os.path.join(rules_root, stage)
            rule_file = file_loader.load_file(rule_file_path)
            if "error" in rule_file:
                self._rules.append({"stage" : stage.removesuffix(".md"), "content" : stage.removesuffix(".md")})
            else:
                self._rules.append({"stage" : rule_file["name"].removesuffix(".md"), "content" : rule_file["content"]})
        return self._rules
    
    def get_rules(self):
        return self._rules
    
    def get_rules_by_stage(self, stage: str):
        for rule in self._rules:
            if rule["stage"] == stage:
                return rule["content"]
        return None