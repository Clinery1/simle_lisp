# About this language
This little language is dynamically typed, fully mutable, and designed for scripting instead of
full programs. I might fork this language to make a more complete language, but this is simply
"how fast can I go from zero to FizzBuzz"

# Inspiration
Scheme, Clojure, and Lua (kinda), and more

# What about syntax highlighting?
I wrote a tree-sitter grammar! It is located [here](https://github.com/Clinery1/simple_lisp-tree-sitter)!

# Aren't there enough parenthesis in a normal-looking language?
Probably, but WE NEEDED MORE! It does get tedious after a while, but it is easier than having
multiple different kinds of character pairs to match across multiple lines. It is easier to match up
the different parenthesis with rainbow parens.

# Isn't it easier to have a normal-looking syntax?
Yes and no. It is harder on the parsing, but maps more closely to the AST if designed correctly.
For this language, an s-expression language makes certain things easier, like defining overloaded
functions. Recursion is also more obvious/intuitive (I think) than in a normal language. With the
s-expression syntax, recursion can use the special `recur` variable to call the current function,
but that is just a language feature not a syntax feature.

# Is there a REPL?
~Yes! I just finished it (at the time of writing) and I also made an Asciinema video of it:~
~[![asciicast](https://asciinema.org/a/659949.svg)](https://asciinema.org/a/659949)~

I have since updated the REPL to include tree-sitter syntax highlighting, a better editor, and a
history buffer
[![asciicast](https://asciinema.org/a/660067.svg)](https://asciinema.org/a/660067)
