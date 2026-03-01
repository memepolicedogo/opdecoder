import json

def main():
    s = set()
    ins = json.load(open("reduced.json"))
    for i in ins:
        for p in i:
            for b in p['opcode'].split(' '):
                if ':' in b:
                    s.add(b)
    for x in s:
        print(x)

if __name__  == "__main__":
    main()
