#!/bin/python
import json
import sys
import os
import multiprocessing as mp
# Specify how many threads to use
THREADS=os.process_cpu_count()

def parse_normal(codes, operands):
    formatted = []
    # Split out operand encoding
    ops = dict()
    if operands:
        for row in operands[1:]:
            ops[row[0]] = row[1:]

    for row in codes[1:]:
        try:
            code = row[0].replace('\n', ' ')
            ins = row[1].replace('\n', ' ')
            # Handle Op/En
            op = ops.get(row[2])
            # Handle Desciption
            desc = row[-1].replace('\n', ' ')
            formatted.append({
                "opcode":code, 
                "instruction": ins, 
                "operands": op, 
                "description": desc
            })
        except:
            if row[0] != "NOTES:":
                print(f"Exception on row {row}")
            continue

    return formatted

def parse_combined(codes, operands):
    formatted = []
    # Split out operand encoding
    ops = dict()
    if operands:
        for row in operands[1:]:
            ops[row[0]] = row[1:]

    for row in codes[1:]:
        try:
            # Split opcode and instruction out
            lines = row[0].split('\n')
            code = lines.pop(0)
            # FUCK YOU AESDECWIDE128KL!!!!
            if code[-1] == '-':
                lines.insert(0, code.split(' ')[-1][:-1])
                code = code[:code.rindex(' ')]
            ins = ''
            for line in lines:
                ins += line
            # Handle Op/En
            op = ops.get(row[1])
            # Handle Desciption
            desc = row[-1].replace('\n', ' ')
            formatted.append({
                "opcode":code, 
                "instruction": ins, 
                "operands": op, 
                "description": desc
            })
        except:
            if row[0] != "NOTES:":
                print(f"Exception on row {row}")
            continue

    return formatted

def normalize(tables):
    instructions = []
    i = 0
    while i<len(tables):
        codes = tables[i]
        for n in range(len(codes[0])):
            # Remove stupid shit
            codes[0][n] = codes[0][n].replace('\n', '')
            codes[0][n] = codes[0][n].replace('*', '')
            codes[0][n] = codes[0][n].replace('/', '')
            codes[0][n] = codes[0][n].replace(' ', '')
        operands = None
        if "OpEn" in codes[0]:
            operands = tables[i+1]
        if codes[0][1] == "Instruction":
            instructions.append(parse_normal(codes, operands))
        elif codes[0][0] == "OpcodeInstruction":
            instructions.append(parse_combined(codes, operands))
        # THE FEAR
        elif codes[0][0] == "OpEn":
            print("Unacompanied Op/En")
            print("prev:")
            print(tables[i-1])
        else:
            print("I don't know what this is and im fucking scared dude")
            print(codes)
            print()
            break

        i += 1
        if operands != None:
            i += 1
    return instructions

def main(args):
    if len(args) != 2:
        print('Invalid args, must pass a single JSON file')
        return
    file = args[1]
    data = json.load(open(file))
    cleaned = normalize(data)
    print(f"Total instructions: {len(cleaned)}")
    json.dump(cleaned, open('cleaned.json', 'w'))
    return
    for ins in cleaned:
        print(f"Instruction: {ins[0]['instruction']}")
    return
if __name__ == '__main__':
    main(sys.argv)
