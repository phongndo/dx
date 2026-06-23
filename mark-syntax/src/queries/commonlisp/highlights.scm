; Fallback highlights for tree-sitter-commonlisp.

[
  (comment)
  (block_comment)
] @comment

[
  (str_lit)
  (fancy_literal)
] @string

(format_specifier) @string.special
(path_lit) @string.special
(char_lit) @character

[
  (num_lit)
  (complex_num_lit)
] @number

(nil_lit) @constant.builtin

[
  (kwd_lit)
  (kwd_symbol)
] @constant

(defun_header
  keyword: (defun_keyword) @keyword
  function_name: (_) @function)

(defun_header
  keyword: (defun_keyword) @keyword)

[
  (for_clause_word)
  (accumulation_verb)
] @keyword

(loop_macro "loop" @keyword)

(list_lit
  . value: (sym_lit) @keyword
  (#match? @keyword "^(block|catch|case|ccase|cond|ctypecase|declare|declaim|defclass|defconstant|defpackage|defparameter|defstruct|defvar|do|do\\*|dolist|dotimes|ecase|etypecase|eval-when|flet|function|go|handler-bind|handler-case|if|labels|lambda|let|let\\*|load-time-value|locally|macrolet|multiple-value-bind|multiple-value-call|multiple-value-prog1|progn|progv|quote|return|return-from|setq|symbol-macrolet|tagbody|the|throw|typecase|unwind-protect|when|unless)$"))

[
  "("
  ")"
] @punctuation.bracket

[
  "'"
  "`"
  ","
  ",@"
  "."
] @punctuation.delimiter

[
  "#+"
  "#-"
  "#C"
  "#c"
] @punctuation.special

(self_referential_reader_macro) @punctuation.special
