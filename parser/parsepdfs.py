#!/bin/python
import pdfplumber
import sys
import os
import json
import multiprocessing

def type_table(table):
    match table[0][0][0:5]:
        case 'Opcod':
            return 'Instruction'
        case 'Op/En':
            return 'Operand'
        case _:
            return 'Unknown'

def filter_size(obj):
    if obj.get('object_type', 0) != 'char':
        return True
    return obj.get('size', 0) > 7

def parse(file):
    pdf = pdfplumber.open(file);
    # get base name for JSON output
    filename = file.split('/')[-1]
    filename = filename.replace('.pdf', '')
    print(f"{filename}: " + "Parsing PDF")
    full_data = []
    for page in pdf.pages:
        # Remove superscript
        page = page.filter(filter_size)
        tables = page.extract_tables()
        if len(tables) == 0:
            # No tables? must be just description, skip.
            continue;
        for table in tables:
            if table[0][0] and "Op" in table[0][0]:
                full_data.append(table)
    # Combine extended tables in full_data and link instruction tables to operand encoding
    pdf.close()
    raw_tables = len(full_data)
    print(f"{filename}: " + f"Extracted {raw_tables} tables ")
    print(f"{filename}: " + "Cleaning and combining tables")
    unknown_count = 0
    cleaned = []
    prev = None
    for table in full_data: 
        curr = type_table(table)
        if curr == 'Unknown':
            # Skip unknown tables
            prev = None
            curr = None
            continue
        if (table[0][1][:2] != 'Op' and table[0][2][:2] != 'Op' ):
            cleaned.append(table)
            prev = None
            curr = None
        elif curr == prev: 
            # Remove defenition from table
            table.pop(0)
            # Merge tables
            cleaned[-1].extend(table)
        else: 
            cleaned.append(table)
        prev = curr
    print(f"{filename}: " + f"Unique/Total: {len(cleaned)}/{raw_tables}")
    # total tables - tables ignored - final table count = tables that were merged into others
    print(f"{filename}: " + f"Writing JSON results to {filename}.json")
    json.dump(cleaned, open(f"out/{filename}.json", 'w'))
    print(f"{filename}: " + "Exiting")
    return

def main(args):
    if len(args) == 1:
        print("Path(s) to PDF(s) required")
    else:
        # Build list of PDFs
        pdfs = set()
        for arg in args[1:]:
            if not os.path.exists(arg):
                print(f"Invalid argument: {arg}")
                return
            path = os.path.abspath(arg)
            if os.path.isdir(path):
                for file in os.listdir(path):
                    file = path+'/'+file
                    if os.path.isfile(file) and file.split('.')[-1] == 'pdf':
                        pdfs.add(file)
            elif path.split('.')[-1] != 'pdf':
                print(f"Invalid file type: {arg}")
            else:
                pdfs.add(path)
        # Create output dir
        os.makedirs('out', exist_ok=True)
        # Each PDF is wholly independent so they can each be processed in a unique thread
        procs = []
        for pdf in pdfs:
            procs.append(multiprocessing.Process(target=parse, args=(pdf,)))
        for proc in procs:
            proc.start()
        for proc in procs:
            # Wait for processing to end
            proc.join()
    return
if __name__ == "__main__":
    main(sys.argv)
