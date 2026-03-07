#!/bin/python
import json

def main():
    ins = json.load(open("mar_2_cleaned.json"))
    reduced = []
    for i in ins:
        table = []
        for p in i:
            if "VEX" in p['opcode']:
                continue
            table.append(p)
        if len(table) != 0:
            reduced.append(table)
    json.dump(reduced, open("reduced.json", 'w'))

if __name__  == "__main__":
    main()
