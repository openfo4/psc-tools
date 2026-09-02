ScriptName SelfKeyword

int A = 1

;@run: => 1
int function SelfKeywordAsValue()
    return GetA(self)
endFunction

;@run: => 1
int function SelfKeywordLoadPlace()
    return self.A
endFunction

;@run: a = 2 => 2
int function SelfKeywordStorePlace(int a)
    self.A = a
    return A
endFunction

int function GetA(SelfKeyword s)
    return s.A
endFunction
