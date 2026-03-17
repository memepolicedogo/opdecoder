#!/bin/python
import json
BASE_FILE = "x64_reduced.json"
OUTPUT_FILE = "x86_reduced.json"

def main():
    ins = json.load(open(BASE_FILE))
    reduced = []
    for i in ins:
        table = []
        for p in i:
            if "REX" in p['opcode'] or p['legacy_support'] != 'V':
                continue
            table.append(p)
        if len(table) != 0:
            reduced.append(table)
    json.dump(reduced, open(OUTPUT_FILE, "w"))

if __name__ == "__main__":
    main()
