#!/bin/python
import pdfplumber
import sys
import json

def type_table(table):
    match table[0][0]:
        case 'Opcode':
            return 'Instruction'
        case 'Opcode/Instruction':
            return 'Instruction'
        case 'Op/En':
            return 'Operand'
        case _:
            return 'Unknown'


def parse(file):
    pdf = pdfplumber.open(file);
    full_data = []
    for page in pdf.pages:
        tables = page.extract_tables()
        if len(tables) == 0:
            # No tables? must be just description, skip.
            continue;
        for table in tables:
            if table[0][0] and "Op" in table[0][0]:
                full_data.append(table)
    # Combine extended tables in full_data and link instruction tables to operand encoding
    pdf.close()
    cleaned = []
    prev = None
    for table in full_data:
        curr = type_table(table)
        if curr == 'Unknown':
            # Skip unknown tables
            prev = None
            curr = None
            continue
        if curr == prev:
            # Remove defenition from table
            table.pop(0)
            # Merge tables
            cleaned[-1].extend(table)
        else:
            cleaned.append(table)
        prev = curr
    json.dump(cleaned, open('out.json', 'w'))
    return

def main(args):
    for arg in args[1:]:
        parse(arg)
    return
if __name__ == "__main__":
    main(sys.argv)
