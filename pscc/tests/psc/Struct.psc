ScriptName StructTest

struct MyStruct
    int x
    float y
    bool z
endStruct

function MakeAndCheckStruct()
    MyStruct s = new MyStruct
    IsAllTruthy(s)
endFunction

bool function IsAllTruthy(MyStruct s)
    if s == none
        return false
    endIf

    if s.x == 0
        return false
    endIf

    if s.y == 0.0
        return false
    endIf

    if !s.z
        return false
    endIf

    return true
endFunction

bool function IsAllTruthyArray(MyStruct[] arr)
    int i = 0
    while i < arr.Length
        if !IsAllTruthy(arr[i])
            return false
        endIf
    endWhile

    return true
endFunction
