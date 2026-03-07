#!/bin/python
import json
def main():
    ins  = json.load(open("full.json"))
    s = set()
    for i in ins:
        for p in i:
            if "VEX" in p['opcode']:
                s.add(p['opcode'].split(' ')[0])
    a = list(s)
    a.sort()
    for c in a:
        print(c)

if __name__ == "__main__":
    main()
