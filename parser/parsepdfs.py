#!/bin/python
import pdfplumber
import sys
import os
import json
import threading

def type_table(table):
    match table[0][0][0:5]:
        case 'Opcod':
            return 'Instruction'
        case 'Op/En':
            return 'Operand'
        case _:
            return 'Unknown'


def parse(file):
    pdf = pdfplumber.open(file);
    # get base name for JSON output
    filename = file.split('/')[-1]
    filename.replace('.pdf', '')
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
            print("--UNKNOWN--")
            print(table)
            print("-----------")
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
    json.dump(cleaned, open(f"out/{filename}.json", 'w'))
    return

def main(args):
    if len(args) == 1:
        print("Path(s) to PDF(s) required")
    else:
        # Create output dir
        os.makedirs('out', exist_ok=True)
        # Each PDF is wholly independent so they can each be processed in a unique thread
        threads = []
        for arg in args[1:]:
            threads.append(threading.Thread(target=parse, args=(arg)))
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
    return
if __name__ == "__main__":
    main(sys.argv)
