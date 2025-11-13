import re
import json
from typing import Dict, List

class Deduplicate:
    def __init__(self, api_key : str):
        self._markdown_files = {}
        
    def deduplicate(self, file_vault: Dict[str, Dict[str, str]]):
        # Keep track the file path of each file
        file_paths_by_id = {}
        # Tokenize file with unique key
        file_by_id = {}
        index = 0
        for path, file in file_vault.items():
            file_paths_by_id[index] = path
            # Try to deduplicate paragraph and strings inside a file
            deduplicated_file = self.__deduplicate_paragraphs(file["content"])
            file_by_id[index] = deduplicated_file
            index += 1

        # Compare each file to one another.
        file_by_id = self.__deduplicate_strings(file_by_id)
        deduplicated_file_by_path = {}

        # Store the path and the dedupliacted file
        for key, path in file_paths_by_id.items():
            if key in file_by_id:
                deduplicated_file_by_path[path] = file_by_id[key]
            else: 
                deduplicated_file_by_path[path] = None

        return deduplicated_file_by_path
    
    def __call_ai(self, id_1: int, id_2: int, string_1: str, string_2: str):
        return { "action" : "keep"}
    

    # Deduplicate sentenses within the same paragraph
    def __deduplicate_sentences(self, paragraph: str):
        # Tokenize string separated by "."
        tokens = self.__tokenizing_text(paragraph, ".")
        # Compare strings
        tokens = self.__deduplicate_strings(tokens)
        # Merge back into one paragraph.
        return self.__merge_text(tokens, ". ")


    # Take in a HashMap and compare in each to every other items
    def __deduplicate_strings(self, tokens: Dict[int, str]):
        # compare each pair of strings
        for i in range(len(tokens) - 1):
            # Skip if string is empty or none
            if not i in tokens or not tokens[i]:
                continue

            for j in range(i + 1, len(tokens)):
                # Skip if string is empty or none
                if not j in tokens or not tokens[j]:
                    continue

                # Getting comparing result from AI
                compare_result = self.__call_ai(i, j, tokens[i], tokens[j])
                if "action" in compare_result:
                    # AI will return "keep" if both strings are not similar
                    if compare_result["action"] == "keep":
                        continue

                    # "Delete" if two strings have similarity > 0.9
                    # Delete the string that appear after the other string to keep one unique copy.
                    elif compare_result["action"] == "delete":
                        tokens[j] = None

                    # "merge" when two strings have similarity >= 0.8 and < 0.9
                    # "merge_content" is the summary of two strings
                    elif compare_result["action"] == "merge":
                        if "merge_content" in compare_result:
                            tokens[j] = None
                            tokens[i] = compare_result["merge_content"] 
        return tokens
        
    

    def __try_parse_int(integer):
        try:
            return int(integer)
        except ValueError:
            return None
        
    # Compare and deduplicate paragraphs with in a file
    def __deduplicate_paragraphs(self, file: str):
        # Tokenize by a new line
        tokens= self.__tokenizing_text(file, "\n")
        # Try to clean up sentences in the same paragraph before comparing
        for key, value in tokens.items():
            tokens[key] = self.__deduplicate_sentences(value)
        # Comparing
        tokens = self.__deduplicate_strings(tokens)
        return self.__merge_text(tokens, "\n")

    # Merge items of a HashMap into one text
    def __merge_text(self, deduplicated_text: Dict[int, str], separate_string: str = " ") -> str:
        content = ""
        for id, value in deduplicated_text.items():
            if value is not None:
                content += value + separate_string

        if content:
            content = content[:-len(separate_string)]
        return content
    
    # Break text into smaller chunks
    def __tokenizing_text(self, text: str, token_char: str):
        tokens = [token.strip() for token in text.split(token_char) if token.strip()]
        token_map = {}
        id = 0
        for token in tokens:
            token_map[id] = token
            id += 1
        return token_map
