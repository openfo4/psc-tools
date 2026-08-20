ScriptName ShortCircuit

int A = 0
int B = 0
int C = 0

;@run: x = 0, y = 0 => 001
;@run: x = 0, y = 1 => 001
;@run: x = 1, y = 0 => 011
;@run: x = 1, y = 1 => 111
int function ShortCircuitAnd(int x, int y)
    int res = 0
    if IsOneThenSetA(x) && IsOneThenSetB(y)
        res = 100
    endIf

    return A + B + res
endFunction

;@run: x = 0, y = 0 => 011
;@run: x = 0, y = 1 => 111
;@run: x = 1, y = 0 => 101
;@run: x = 1, y = 1 => 101
int function ShortCircuitOr(int x, int y)
    int res = 0
    if IsOneThenSetA(x) || IsOneThenSetB(y)
        res = 100
    endIf

    return A + B + res
endFunction

;@run: x = 0, y = 0, z = 0 => 0011
;@run: x = 0, y = 0, z = 1 => 0011
;@run: x = 0, y = 1, z = 0 => 0111
;@run: x = 0, y = 1, z = 1 => 1111
;@run: x = 1, y = 0, z = 0 => 1001
;@run: x = 1, y = 0, z = 1 => 1001
;@run: x = 1, y = 1, z = 0 => 1001
;@run: x = 1, y = 1, z = 1 => 1001
int function ShortCircuitPrecedence(int x, int y, int z)
    int res = 0
    if IsOneThenSetA(x) || IsOneThenSetB(y) && IsOneThenSetC(z)
        res = 1000
    endIf

    return A + B + C + res
endFunction

;@run: x = 0, y = 0, z = 0 => 0011
;@run: x = 0, y = 0, z = 1 => 0011
;@run: x = 0, y = 1, z = 0 => 0111
;@run: x = 0, y = 1, z = 1 => 1111
;@run: x = 1, y = 0, z = 0 => 0101
;@run: x = 1, y = 0, z = 1 => 1101
;@run: x = 1, y = 1, z = 0 => 0101
;@run: x = 1, y = 1, z = 1 => 1101
int function ShortCircuitPrecedenceParens(int x, int y, int z)
    int res = 0
    if (IsOneThenSetA(x) || IsOneThenSetB(y)) && IsOneThenSetC(z)
        res = 1000
    endIf

    return A + B + C + res
endFunction

bool function IsOneThenSetA(int x)
    A = 1
    return x == 1
endFunction

bool function IsOneThenSetB(int x)
    B = 10
    return x == 1
endFunction

bool function IsOneThenSetC(int x)
    C = 100
    return x == 1
endFunction
