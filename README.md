# Hi
This repo is an awful mess. I'm gonna clean it up someday I promise. If you want to try the disassembler its in decoder/. Its a rust project so you'll need all the rust stuff to build it.
## Running the disassembler
The text below is a LIE that is only true if not an ELF executable, if you are trying to disassembly an ELF then just pass the path with the `-i` flag and you'll be chill\
Currently it can't parse executable headers of any kind, I'm working on that, so you'll have to do some stupid annoying stuff.\
The command you probably want is 
```
{path to decoder} -t {path to tree2.json} -i {path to your executable} --offset {number of bytes before the text section} -m {length of text section in bytes}
```
This will print out each line as disassembled. Oh yeah also it only works for 64 bit code and no vector extension instructions.
## Parser
This is a collection of python scripts I used to get the JSON with all the x86 instructions. I'm keeping them for postarity, someday I might merge them into a single script or something. I would not recomend trying to build your own JSON and tree because the intel docs are full of mistakes I had to fix by hand, and you'll have to fix them too, but you won't have spent three days previously slamming your head against the docs so it'll be much harder. Maybe I'll add some notes to that dirs README that say what's wrong with the docs and how to fix them by hand but I don't want to now.

