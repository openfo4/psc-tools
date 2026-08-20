ScriptName PropertyTest

int Property AutoProp = 10 auto

;@run: => 10
int function GetAutoProp()
    return AutoProp
endFunction

;@run: x = 20 => 20
int function SetAndGetAutoProp(int x)
    AutoProp = x
    Barrier()
    return AutoProp
endFunction

; Compiler barrier
function Barrier()
endFunction
