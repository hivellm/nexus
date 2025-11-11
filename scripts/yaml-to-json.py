#!/usr/bin/env python3
"""Convert YAML to JSON"""
import yaml
import json
import sys

if len(sys.argv) != 3:
    print("Usage: yaml-to-json.py <input.yaml> <output.json>")
    sys.exit(1)

input_file = sys.argv[1]
output_file = sys.argv[2]

try:
    with open(input_file, 'r', encoding='utf-8') as f:
        data = yaml.safe_load(f)
    
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    
    print(f"✅ Converted {input_file} to {output_file}")
except Exception as e:
    print(f"❌ Error: {e}")
    sys.exit(1)

