;@import: ImportB.psc
ScriptName ImportA

;@run: x = 4 => 16
function CallImportPow2(int x) global
    return B.Pow2(x)
endFunction
