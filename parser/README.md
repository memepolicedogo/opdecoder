# Parsing
Creates intermediate JSON representations of instruction tables from the Intel SDM PDFs.\
These tables are joined across pages so each instruction should have a single instruction table and a single operand table, this is a pretty basic script so artifacts owing to inconsistancies in Intel's documentation or limitations of the PDF reader aren't normalized out.\
The input PDFs should only be the instruction definitions from the SDM (e.g. pgs. 119-697 from Vol 2A), parsing the full volume may cause unexpected behavior and will slow down the process significantly.\
## Usage
Accepts paths to either specific PDFs or directories, non PDFs (based on extension) in given directories will be ignored
```
./parsepdfs.py {paths to PDF(s)...}
```
For each PDF it produces a `{PDF Name}.json` file in `./out/`
## Optimization
Each PDF is parsed in a seperate process, so splitting up the larger PDFs will allow you to take advantage of your (presumably) multi core CPU. It likely isn't worth the time to split up the PDFs more than the 4 parts Intel provides, especily because you have to be careful not to split any instruction/opperand tables. That said, I'm probably gonna implement automatic optimization of this kind in the future because it'd be fun.
# Merging
This is a super simple script that just takes the JSON generated in `./out/` and combines them into a single JSON file for portability.
## Usage
```
./mergejson.py
```
Make sure you run it in the same directory as you ran `./parsepdfs.py`, NOT in `./out`. 
