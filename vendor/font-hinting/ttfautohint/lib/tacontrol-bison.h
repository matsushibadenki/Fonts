/* A Bison parser, made by GNU Bison 3.0.4.  */

/* Skeleton interface for Bison GLR parsers in C

   Copyright (C) 2002-2015 Free Software Foundation, Inc.

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with this program.  If not, see <http://www.gnu.org/licenses/>.  */

/* As a special exception, you may create a larger work that contains
   part or all of the Bison parser skeleton and distribute that work
   under terms of your choice, so long as that work isn't itself a
   parser generator using the skeleton or a modified version thereof
   as a parser skeleton.  Alternatively, if you modify or redistribute
   the parser skeleton itself, you may (at your option) remove this
   special exception, which will cause the skeleton and the resulting
   Bison output files to be licensed under the GNU General Public
   License without this special exception.

   This special exception was added by the Free Software Foundation in
   version 2.2 of Bison.  */

#ifndef YY_TA_CONTROL_LIB_TACONTROL_BISON_H_INCLUDED
# define YY_TA_CONTROL_LIB_TACONTROL_BISON_H_INCLUDED
/* Debug traces.  */
#ifndef YYDEBUG
# define YYDEBUG 0
#endif
#if YYDEBUG
extern int TA_control_debug;
#endif
/* "%code requires" blocks.  */
#line 43 "lib/tacontrol.bison" /* glr.c:197  */

#include "ta.h"
#include "tashaper.h"

/* we don't change the name prefix of flex functions */
#define TA_control_lex yylex

#line 52 "lib/tacontrol-bison.h" /* glr.c:197  */

/* Token type.  */
#ifndef YYTOKENTYPE
# define YYTOKENTYPE
  enum yytokentype
  {
    EOE = 258,
    INTEGER = 259,
    INTERNAL_FLEX_ERROR = 260,
    INVALID_CHARACTER = 261,
    LEFT = 262,
    NAME = 263,
    NODIR = 264,
    POINT = 265,
    REAL = 266,
    RIGHT = 267,
    TOUCH = 268,
    WIDTH = 269,
    XSHIFT = 270,
    YSHIFT = 271
  };
#endif

/* Value type.  */
#if ! defined YYSTYPE && ! defined YYSTYPE_IS_DECLARED

union YYSTYPE
{
#line 51 "lib/tacontrol.bison" /* glr.c:197  */

  char character;
  Control_Type type;
  long integer;
  char* name;
  number_range* range;
  double real;
  Control* control;

#line 91 "lib/tacontrol-bison.h" /* glr.c:197  */
};

typedef union YYSTYPE YYSTYPE;
# define YYSTYPE_IS_TRIVIAL 1
# define YYSTYPE_IS_DECLARED 1
#endif

/* Location type.  */
#if ! defined YYLTYPE && ! defined YYLTYPE_IS_DECLARED
typedef struct YYLTYPE YYLTYPE;
struct YYLTYPE
{
  int first_line;
  int first_column;
  int last_line;
  int last_column;
};
# define YYLTYPE_IS_DECLARED 1
# define YYLTYPE_IS_TRIVIAL 1
#endif



int TA_control_parse (Control_Context* context);

#endif /* !YY_TA_CONTROL_LIB_TACONTROL_BISON_H_INCLUDED  */
