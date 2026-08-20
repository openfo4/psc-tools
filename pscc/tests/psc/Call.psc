ScriptName Call

int function A(int x) global
    return x * 2
endFunction

;@run: x = 4 => 8
int function CallGlobal(int x)
    return Call.A(x)
endFunction

int function B(int x)
    return x + 1
endFunction

;@run: x = 2 => 3
int function CallMethod(int x)
    return B(2)
endFunction
