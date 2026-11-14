class Instructions:

function IntOps():
    x = iadd x y
    x = isub x y
    x = imul x y
    x = idiv x y
    x = imod x y
    x = ineg x

function FloatOps():
    x = fadd x y
    x = fsub x y
    x = fmul x y
    x = fdiv x y
    x = fneg x

function Compare():
    x = cmpeq x y
    x = cmplt x y
    x = cmple x y
    x = cmpgt x y
    x = cmpge x y

function ControlFlow():
    jmp 1
    jmp -1
    jmpt x 2
    jmpt x -2
    jmpf x 3
    jmpt x 4
    ret x

function Call():
    x = call_method y MyFunc a b c
    x = call_parent MyFunc a b c
    x = call_static MyClass MyFunc a b c

function Property():
    x = property_get y MyProp
    property_set MyProp x y

function Array():
    x = array_create y
    x = array_length y
    x = array_get y z
    array_set x y z
    x = array_find y z i
    x = array_rfind y z i
    x = array_find_struct y MyMember z i
    x = array_rfind_struct y MyMember z i
    array_add x y z
    array_insert x y z
    array_pop x
    array_remove x y z
    array_clear x

function Struct():
    x = struct_create
    x = struct_get y MyMember
    struct_set x MyMember y

function Other():
    nop
    x = string_concat y z
    x = is y MyClass
    x = cast y
    x = not y
    x = assign y
