"""Reconstruct visible PTY text; stripping ANSI loses retained character cells."""
import re
import unicodedata
TOKEN = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]|\x1b\].*?(?:\x07|\x1b\\)|\x1bP.*?\x1b\\|\x1b[()][\w]|\x1b.', re.S)


def screen_text(output, rows=40, columns=140):
    screen = [[' '] * columns for _ in range(rows)]
    row = column = 0
    saved = (0, 0)
    def text(value):
        nonlocal row, column
        for char in value:
            if char == '\r': column = 0
            elif char == '\n': row += 1
            elif char == '\b': column = max(0, column - 1)
            elif char == '\t': column = min(columns - 1, (column // 8 + 1) * 8)
            elif char.isprintable() and not unicodedata.combining(char):
                width = 2 if unicodedata.east_asian_width(char) in ['W', 'F'] else 1
                if column >= columns: column = 0; row += 1
                if row >= rows: screen.pop(0); screen.append([' '] * columns); row = rows - 1
                screen[row][column] = char
                if width == 2 and column + 1 < columns: screen[row][column + 1] = ''
                column += width
    source = output.decode(errors='replace') if isinstance(output, (bytes, bytearray)) else output
    position = 0
    for match in TOKEN.finditer(source):
        text(source[position:match.start()]); position = match.end()
        token = match.group()
        if not token.startswith('\x1b['): continue
        command = token[-1]; parameters = token[2:-1]
        if parameters.startswith('?'): continue
        values = [int(value) if value.isdigit() else 0 for value in parameters.split(';')]
        amount = values[0] or 1
        row = min(max(0, row), rows - 1); column = min(max(0, column), columns - 1)
        if command in ['H', 'f']:
            row = min(rows - 1, amount - 1); column = min(columns - 1, (values[1] or 1) - 1) if len(values) > 1 else 0
        elif command == 'A': row = max(0, row - amount)
        elif command == 'B': row = min(rows - 1, row + amount)
        elif command == 'C': column = min(columns - 1, column + amount)
        elif command == 'D': column = max(0, column - amount)
        elif command == 'G': column = min(columns - 1, amount - 1)
        elif command == 'd': row = min(rows - 1, amount - 1)
        elif command == 's': saved = (row, column)
        elif command == 'u': row, column = saved
        elif command == 'K':
            start = 0 if values[0] in [1, 2] else column
            end = column + 1 if values[0] == 1 else columns
            screen[row][start:end] = [' '] * (end - start)
        elif command == 'J':
            if values[0] in [2, 3]: screen = [[' '] * columns for _ in range(rows)]
            elif values[0] == 0:
                screen[row][column:] = [' '] * (columns - column)
                for index in range(row + 1, rows): screen[index] = [' '] * columns
            elif values[0] == 1:
                for index in range(row): screen[index] = [' '] * columns
                screen[row][:column + 1] = [' '] * (column + 1)
        elif command == 'X': screen[row][column:column + amount] = [' '] * min(amount, columns - column)
    text(source[position:])
    return '\n'.join(''.join(line).rstrip() for line in screen)
