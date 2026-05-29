import turtle
import math

t = turtle.Turtle()
t.speed(0)
turtle.bgcolor("black")
t.hideturtle()
turtle.colormode(255)

FONT = ("Courier", 40, "bold")
CHAR_W = 30

def draw_rainbow_text(text):
    n = len(text)
    total_w = n * CHAR_W
    start_x = -total_w / 2

    t.penup()
    t.goto(start_x, -20)
    t.pendown()

    for i, ch in enumerate(text):
        hue = i / max(n - 1, 1)
        r = int(255 * (0.5 + 0.5 * math.sin(hue * 2 * math.pi)))
        g = int(255 * (0.5 + 0.5 * math.sin(hue * 2 * math.pi + 2 * math.pi / 3)))
        b = int(255 * (0.5 + 0.5 * math.sin(hue * 2 * math.pi + 4 * math.pi / 3)))
        t.pencolor(r, g, b)
        t.write(ch, font=FONT)
        t.penup()
        t.goto(start_x + (i + 1) * CHAR_W, -20)
        t.pendown()

text = turtle.textinput("Rainbow Text", "Enter your text:")
if text:
    draw_rainbow_text(text)
    turtle.done()
else:
    turtle.bye()
