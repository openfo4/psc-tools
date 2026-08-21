ScriptName GlobalTest

int MyGlobal = 12
int MyConstGlobal = 123 const

;@run: => 12
int function GetMyGlobal()
    return MyGlobal
endFunction

;@run: x = 2 => 14
int function SetAndGetMyGlobal(int x)
    MyGlobal += x
    Barrier()
    return MyGlobal
endFunction

; Compiler barrier
function Barrier()
endFunction
