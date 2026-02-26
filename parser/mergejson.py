#!/bin/python
import os
import json

def main():
    if not os.path.exists('./out'):
        print("No ./out/ directory found")
        return
    full_data = []
    for file in os.listdir('./out/'):
        if file.split('.')[-1] == 'json':
            full_data.extend(json.load(open('out/'+file)))
    json.dump(full_data, open('instructions.json', 'w'))

if __name__ == '__main__':
    main()
