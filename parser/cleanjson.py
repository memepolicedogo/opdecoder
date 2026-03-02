#!/bin/python
import json
import sys
import os
import multiprocessing as mp
# Specify how many threads to use
THREADS=os.process_cpu_count()
KNOWN_TOKENS = [
    "/0",
    "/1",
    "/2",
    "/3",
    "/4",
    "/5",
    "/6",
    "/7",
    "rb",
    "rw",
    "rd",
    "ro",
    "ib",
    "iw",
    "id",
    "io",
    "cb",
    "cw",
    "cd",
    "cp",
    "co",
    "NP",
    "NFx",
    "/r",
    "/is4",
    "/vsib",
    "i",
]


def is_expected_token(string: str):
    # Should be the default case
    if is_byte(string):
        return True
    # Common prefixes
    elif "VEX" in string or "REX" in string:
        return True
    # Common non-standard tokens
    elif string in KNOWN_TOKENS:
        return True
    # That one format
    elif ":" in string:
        return True
    # Combined byte suffix
    elif len(string) > 3 and ('+' in string or '/' in string):
        return is_byte(string[:2]) and string[3:] in KNOWN_TOKENS
    return False
    return is_byte(string) or "VEX" in string or "REX" in string or string in KNOWN_TOKENS or ':' in string or string.endswith('+i')

def is_byte(string):
    if len(string) != 2:
        return False
    try:
        x = int(string, 16)
        return True
    except:
        return False

def parse_normal(codes, operands, mode_row):
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
            # Handle 32/64 bit mode
            six = row[mode_row]
            three = row[mode_row+1]
            # Handle Desciption
            desc = row[-1].replace('\n', ' ')
            formatted.append({
                "opcode":code, 
                "instruction": ins, 
                "operands": op, 
                "current_support": six,
                "legacy_support": three,
                "description": desc
            })
        except:
            if row[0] != "NOTES:":
                print(f"Exception on row {row}")
            continue

    return formatted

def parse_combined(codes, operands, mode_row):
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
            six = 'V'
            three = 'V'
            if '/' not in row[mode_row]:
                six = row[mode_row]
                three = row[mode_row+1]
            else:
                tmp = row[mode_row].split('/')
                six = tmp[0]
                three = tmp[1]
            # Handle Desciption
            desc = row[-1].replace('\n', ' ')
            formatted.append({
                "opcode":code, 
                "instruction": ins, 
                "operands": op, 
                "current_support": six,
                "legacy_support": three,
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
        mode_row = 3
        for n in range(len(codes[0])):
            if "64" in codes[0][n]:
                mode_row = n
            # Remove stupid shit
            codes[0][n] = codes[0][n].replace('\n', '')
            codes[0][n] = codes[0][n].replace('*', '')
            codes[0][n] = codes[0][n].replace('/', '')
            codes[0][n] = codes[0][n].replace(' ', '')
        operands = None
        new = []
        if "OpEn" in codes[0]:
            operands = tables[i+1]
        if codes[0][1] == "Instruction":
            new = parse_normal(codes, operands, mode_row)
        elif codes[0][0] == "OpcodeInstruction":    
            new = parse_combined(codes, operands, mode_row)
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
        # Clean instruction format
        for ins in new:
            # Support
            if len(ins['current_support']) > 4:
                ins['current_support'] = ins['current_support'][0]
            if len(ins['legacy_support']) > 4:
                ins['legacy_support'] = ins['legacy_support'][0]
            # Opcode
            code = ins['opcode'].split(' ')
            prev = code[0]
            clurned = f"{prev}"
            if not is_expected_token(prev):
                print(f"Unexpected token at #{len(instructions)}:")
                print(f"Token: {prev}")
                print(f"Instruction: {ins['instruction']}")
            for curr in code[1:]:
                if curr == '+' and "REX" in prev:
                    continue
                elif curr.startswith('+r') and is_byte(prev):
                    clurned += f"{curr}"
                    continue
                if not is_expected_token(curr):
                    print(f"Unexpected token at #{len(instructions)}:")
                    print(f"Token: {curr}")
                    print(f"Instruction: {ins['instruction']}")
                clurned += f" {curr}"
                prev = curr
            ins['opcode'] = clurned
        instructions.append(new)
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
