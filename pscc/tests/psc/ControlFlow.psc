ScriptName ControlFlow

;@run: x = 10 => 45
int function CountTo(int x)
    int i = 0
    int r = 0

    while i < x
        r += i
        i += 1
    endWhile

    return r
endFunction

;@run: x = 3 => 4
;@run: x = 4 => 8
;@run: x = 1 => 0
int function ElseIfBasic(int x)
    if x == 3
        x += 1
    elseIf x == 4
        x *= 2
    else
        x = 0
    endIf

    return x
endFunction

;@run: x = 100 => 4
;@run: x = 200 => 3
;@run: x = 101 => 2
;@run: x = 150 => 1
int function NestedIf(int x)
    if x > 100
        if x < 200
            if x == 150
                x = 1
            else
                x = 2
            endIf
        else
            x = 3
        endIf
    else
        x = 4
    endIf

    return x
endFunction
