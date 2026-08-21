ScriptName CallComplex

CallComplex Property MyProp auto
CallComplex MyGlobal
CallComplex[] MyArray

function CallInArray()
    MyArray[0].F()
endFunction

function CallInGlobal()
    MyGlobal.F()
endFunction

function CallInProperty()
    MyProp.F()
endFunction

function F()
endFunction
