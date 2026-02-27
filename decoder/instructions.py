#!/bin/python

class Opcode:
    prefixes = [
        ""
    ]

    literals = [
            'c8'
    ]

    suffix = range(58, 58+7)


    def __eq__(self, val: object, /) -> bool:
        if type(val) != str:
            return False
        hexb = val.split(' ')
        if hexb[0] in self.prefixes:
            hexb.pop(0)
        try:
            for i in range(len(self.literals)):
                byt = hexb.pop(0)
                if byt != self.literals[i]:
                    return False
        except IndexError:
            # If the input is fewer bytes than the max of the instruction it's considered equal
            return True
        except:
            return False
        if len(hexb) != 0 and self.suffix:
            if int(hexb[0], 16) in self.suffix:
            # Check suffix (masked) byte
                return True
            return False
        return True

    def __hash__(self):
        return hash(self.literals)

class InstructionSet:
    prefixes = [
        "",
    ]
    codes = {
        '0a': ""
    }

