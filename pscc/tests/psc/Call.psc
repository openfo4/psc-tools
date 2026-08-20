ScriptName Call

int function A(int x) global
    return x * 2
endFunction

;@run: x = 4 => 8
int function CallGlobal(int x)
    return Call.A(x)
endFunction
