# "Vtables" (Done)
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


# Maps/Objects (Done)
An `IdentMap<Data>` to store things. Also include an `intern` and `getString` function to convert
between interned and regular strings.
Example:
```simplelisp
(def pi 3.1415) ; *EXACTLY* pi without question
(def myObject (object
    .pi
    (.myName "Clinery")))
```


# Modules (In progress)
These are literally just an object with the properties set on them.
Example:
```simplelisp
(module example)
(println example/helloWorld)    ; > "Hello, world!"
(println example)               ; > <object>
```

Modules can either be a file or a folder with a `mod.slp` file in it. Just like with Rust modules.


# Document what stuff is pass-by-value or pass-by-reference (TODO; needs more information)
There are a few options:
- Simply document the existing behavior
- Change everything to pass-by-reference and have possible bugs from accidentally mutating something
    we shouldn't mutate.
- Change everything to pass-by-value and have lots of clones. That will affect performance because
    each thing will have to be deep copied, or I can have a "clone-on-write" flag that can be set in
    the given `DataBox` or whatever I change to.


# Data storage rewrite (TODO)
Probably store small values like `Number`, `Float`, `Character`, etc. in a tagged pointer.

## Easier rewrite
Just change how data is stored, and add an `ensure_heap_allocation` method on `DataRef` to make it
heap allocate. Keep most of the API the same, but require more passing of `DataStore` to actually
allocated things.
Possibly rewrite `Vec` to use memory allocated through `DataStore` or something.

## Easy/Radical rewrite compromise
Add a "raw memory" API to let the program allocate and use arbitrary memory buffers.
Implement everything in the "Easier rewrite" section.

## Radical rewrite
Make the language deal with pointers, initialization, data layout, and other `unsafe` things.

This solution is obviously going to be a radical rewrite of basically the entire language, but it
would make future things... Maybe easier? I could implement a GC, dynamic allocated things, etc,
or I could do a RAII thing and add a destructor for objects, which would then be a language
internal feature. I would also have to do a more complex VM to allow for allocating arbitrary memory
locations.

### Primitive data layout
The primitives would have to have a well-defined in-memory layout that the VM can read/write/modify
any way it wants. Objects would have to have defined types, sizes, etc. in-memory.

### Writing arbitrary memory
It wouldn't be arbitrary, exactly, but we would allocate pages of memory and have an accessor
function that acts like a translation layer from "abstract VM with a linear memory model" to
"non-contiguous list of pages of arbitrary memory"


# Variable rewrite (TODO; depends on [Data storage rewrite])
The `set` instruction no longer takes an `Ident` for the variable, but instead a reference to the
heap-allocated data pointer and sets that. This might work, but it also might not.
