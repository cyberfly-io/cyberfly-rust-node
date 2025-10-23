#!/usr/bin/env python3
import re
import sys

def fix_error_variants(content):
    # Fix DbError::InternalError
    content = re.sub(
        r'DbError::InternalError\(([^)]+)\)',
        r'DbError::InternalError { message: \1, context: None, debug_info: None }',
        content
    )
    
    # Fix DbError::InvalidData
    content = re.sub(
        r'DbError::InvalidData\(([^)]+)\)',
        r'DbError::InvalidData { message: \1, field: None, expected_format: None }',
        content
    )
    
    # Fix DbError::SignatureError
    content = re.sub(
        r'DbError::SignatureError\(([^)]+)\)',
        r'DbError::SignatureError { message: \1, public_key: None, signature: None }',
        content
    )
    
    return content

if __name__ == "__main__":
    with open("src/graphql.rs", "r") as f:
        content = f.read()
    
    fixed_content = fix_error_variants(content)
    
    with open("src/graphql.rs", "w") as f:
        f.write(fixed_content)
    
    print("Fixed error variants in src/graphql.rs")