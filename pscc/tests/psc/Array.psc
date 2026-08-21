ScriptName ArrayTest

int[] MyArray

int function FillAndSum(int size)
    FillArray(size)

    int sum = 0
    int i = 0
    while i < MyArray.Length
        sum += MyArray[i]
    endWhile

    return sum
endFunction

function FillArray(int size)
    MyArray = new int[size]
    
    int i = 0
    while i < MyArray.Length
        MyArray[i] = i
    endWhile
endFunction
