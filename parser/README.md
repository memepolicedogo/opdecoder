# Parsing
Creates intermediate JSON representations of instruction tables from the Intel SDM PDFs.\
These tables are joined across pages so each instruction should have a single instruction table and a single operand table, this is a pretty basic script so artifacts owing to inconsistancies in Intel's documentation or limitations of the PDF reader aren't normalized out.\
The input PDFs should only be the instruction definitions from the SDM (e.g. pgs. 119-697 from Vol 2A), parsing the full volume may cause unexpected behavior and will slow down the process significantly.\
You can use the combined reference & a single PDF, but each PDF is parsed on a seperate thread so it should run faster using seperate PDFs for each volume. If for some reason you really want to optimize parsing speed you can split the PDFs further, as long as you don't break up an instruction (e.g. if an instruction table is on page 123 but the opcode table is on page 124 if you split the pdf into 1-123 and 124-n later proccess won't work properly)
## Usage
```
./parsepdfs.py {paths to PDF(s)...}
```
For each PDF it produces a `{PDF Name}.json` file in `./out/`
