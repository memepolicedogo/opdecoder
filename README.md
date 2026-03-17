# Hi
This repo is an awful mess. I'm gonna clean it up someday I promise. If you want to try the disassembler its in decoder/. Its a rust project so you'll need all the rust stuff to build it.
## Running the disassembler
It will infer where the executable code is based on the headers of the file, though this only currently support ELF and PE files
The command you probably want is 
```
decoder -t {path to tree3.json} -i {path to your executable} 
```
This will print out each line as disassembled. Oh yeah also it only works for 64 bit code and no vector extension instructions.
## Parser
This is a collection of python scripts I used to get the JSON with all the x86 instructions. I'm keeping them for postarity, someday I might merge them into a single script or something. I would not recomend trying to build your own JSON and tree because the intel docs are full of mistakes I had to fix by hand, and you'll have to fix them too, but you won't have spent three days previously slamming your head against the docs so it'll be much harder. Maybe I'll add some notes to that dirs README that say what's wrong with the docs and how to fix them by hand but I don't want to now.

