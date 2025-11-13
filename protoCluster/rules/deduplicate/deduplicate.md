# Deduplicate

## Overview
This process performs multi-level deduplication using semantic embeddings.
You are the AI agent that will determine the merging or summarizing of the given strings.

## Priority: 1

## Rules

1. **Input**
    - You are given two strings of text.
    - The first string with the smaller ID value will be appearing before in the orginal text.
    - The second of string with the larger ID value will be appearing after the other sentnces in the text.

2. **Generate strings Embeddings** 
    - Generate the embeddings of each sentence.

3. **Compute Similarity** 
    - Use **cosine similarity** to compare embeddings.  
    - Similarity threshhold is 0.9

4. **Deduplicate strings with similary greater than or equal to 0.9.**  
    - The second string with the larger ID must be removed.
    - Always return "action" = "delete".
    - Return "ID" = "string_id". Replace "string_id" with the ID numerical value of the string to be deleted.
    - Always the return the action to be performed on the sentence with larger ID numerical value.
    - **Output example**
        {
            "ID" : 0
            "action" : "delete"
        }

4. **Deduplicate strings with similary greater than or equal to 8.0 and less than 0.9 **  
    - Summarize the two strings.
    - Keep as many orginal sentences as possible.
    - Do not abort any idea of the orginal strings.
    - You need to merge two strings together seamlessly.
    - Return "action" = "merge".
    - Return "ID_2" = "string_id". Replace "strin_id" with the ID numerical value of the strin to be deleted. Always delete strin with larger ID
    - Return the summary content of two strings "merge_content" = "summary". Replace "summary" with the summary of two strings.
    - **Output example**
    {
        "ID_1" : 0
        "ID_2" : 0
        "action" : "merge"
        "merge_content" = "summary"
    }

5. **Sentences with similary less then 0.8**  
    - Return "action" = "keep"
    {
        "action" : "keep"
    }

6 **Output**
    - Always return output as object in the specified format.
