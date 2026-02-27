#!/bin/python
import sys

def decode(hexstr):
    return

def main(args):
    if len(args) == 1:
        print("decode -{mode} {file}")
        print("""Modes: 
        -i  decodes user input
        -f  decodes from file
        -F  treats file as hex string
        -l  literal hex string ('file')""")
        return
    elif args[1][0] != '-' or args[1][1] not in 'ifFl':
        print("invalid option")
        return
    match args[1][1]:
        case 'i':
            print()

if __name__ == '__main__':
    main(sys.argv)
