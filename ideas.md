# "Vtables"
A table of functions. Similar to JS's `.prototype` field.
Example:
```simplelisp
(def getAge [person]
    (
(def personVTable (object
    .getAge ; shorthand for (:getAge ageAge)
    .getName))
(defn createPerson [name age]
    (object
        (.$ personVTable)   ; this is the magic bit. If we try to call ANY object, it looks for a
        .age                ; `$` entry in the object, and then it tries to match the first argument
        .name))             ; to an entry in the vTable. If it can't find one, then it tries the
                            ; fields in the object. If it tries the main fields and NOT the vTable,
                            ; then the result IS NOT called. ONLY VTABLE ENTRIES ARE CALLED
                            ; otherwise it is considered a field access.
```


# Maps/Objects
An `IdentMap<Data>` to store things. Also include an `intern` and `getString` function to convert
between interned and regular strings.
Example:
```simplelisp
(def pi 3.1415) ; *EXACTLY* pi without question
(def myObject (object
    .pi
    (.myName "Clinery")))
```


# Modules
These are literally just an object with the properties set on them.
Example:
```simplelisp
(module example)
(println example/helloWorld)    ; > "Hello, world!"
(println example)               ; > <object>
```

Modules can either be a file or a folder with a `mod.slp` file in it. Just like with Rust modules.
