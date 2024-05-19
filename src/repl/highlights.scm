"defn"    @keyword.function
"fn"      @keyword.function
"def"     @keyword
"set"     @keyword
"begin"   @keyword
"cond"    @keyword.conditional
"object"  @keyword
"chain"   @keyword
"module"  @keyword.import

((ident) @operator (#eq? @operator "="))
((ident) @operator (#eq? @operator "!="))
((ident) @operator (#eq? @operator "<"))
((ident) @operator (#eq? @operator "<="))
((ident) @operator (#eq? @operator ">"))
((ident) @operator (#eq? @operator ">="))

((ident) @operator (#eq? @operator "+"))
((ident) @operator (#eq? @operator "-"))
((ident) @operator (#eq? @operator "*"))
((ident) @operator (#eq? @operator "/"))
((ident) @operator (#eq? @operator "%"))

((ident) @operator (#eq? @operator "+="))
((ident) @operator (#eq? @operator "-="))
((ident) @operator (#eq? @operator "*="))
((ident) @operator (#eq? @operator "/="))
((ident) @operator (#eq? @operator "%="))

((ident) @function.builtin (#eq? @function.builtin "gcCollect"))
((ident) @function.builtin (#eq? @function.builtin "and"))
((ident) @function.builtin (#eq? @function.builtin "or"))
((ident) @function.builtin (#eq? @function.builtin "index"))
((ident) @function.builtin (#eq? @function.builtin "list"))
((ident) @function.builtin (#eq? @function.builtin "clone"))
((ident) @function.builtin (#eq? @function.builtin "length"))
((ident) @function.builtin (#eq? @function.builtin "listPop"))
((ident) @function.builtin (#eq? @function.builtin "debug"))
((ident) @function.builtin (#eq? @function.builtin "intern"))

(true)    @boolean
(false)   @boolean

"("       @punctuation.bracket
")"       @punctuation.bracket
"["       @punctuation.bracket
"]"       @punctuation.bracket
"{"       @punctuation.bracket
"}"       @punctuation.bracket

(recur) @function.builtin

(splat "..." @punctuation)

(module (ident) @module)

(defn
  name: (funcName) @function.definition)
(def
  name: (varName) @variable.definition)
(set
  name: (varName) @variable)
(cond_else
  "else" @keyword.conditional)
(chain
  name: (varName) @variable.definition)

(list
  first: (ident) @function.call)
(object
  (dotIdent) @variable.member.definition)
(path
  (pathSeparator) @punctuation.delimiter)
(pathField) @variable.member

(squiggle
  (varName) @variable.parameter)
(vector
  (paramName) @variable.parameter)
(vector
  ("&" @punctuation))

((dotIdent) @punctuation.special (#eq? @punctuation.special ".$"))

(ident) @variable
(dotIdent) @variable.member
(string) @string
(string_escape) @string.escape
(char) @character
(number) @number
(float) @float
(comment) @comment
(docComment) @comment
(replDirective) @keyword
