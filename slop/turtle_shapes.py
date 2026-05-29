import turtle

COLORS = ["red", "blue", "green", "orange", "purple", "cyan", "magenta", "yellow"]

def draw_with_numbers(numbers):
    t = turtle.Turtle()
    t.speed(0)
    t.pensize(2)
    turtle.bgcolor("black")

    for i, num in enumerate(numbers):
        t.pencolor(COLORS[i % len(COLORS)])
        t.forward(num * 5)
        t.right(num)

    t.hideturtle()
    turtle.done()


def draw_flower(numbers):
    t = turtle.Turtle()
    t.speed(0)
    turtle.bgcolor("black")

    for i, num in enumerate(numbers):
        t.pencolor(COLORS[i % len(COLORS)])
        t.pensize(num % 5 + 1)
        t.circle(num)
        t.right(360 / len(numbers))

    t.hideturtle()
    turtle.done()


def draw_polygons(numbers):
    t = turtle.Turtle()
    t.speed(0)
    t.pensize(2)
    turtle.bgcolor("black")

    for i, num in enumerate(numbers):
        t.pencolor(COLORS[i % len(COLORS)])
        sides = max(3, abs(num) % 10)
        length = abs(num) * 3

        for _ in range(sides):
            t.forward(length)
            t.right(360 / sides)

        t.penup()
        t.forward(50)
        t.pendown()

    t.hideturtle()
    turtle.done()


if __name__ == "__main__":
    print("╔══════════════════════════════════════╗")
    print("║       Turtle Number Art              ║")
    print("╚══════════════════════════════════════╝")
    print()
    print("Pick a mode:")
    print("  1 = Path — numbers guide distance & turns")
    print("  2 = Flower — numbers become circle sizes")
    print("  3 = Polygons — numbers become polygon sides")
    print()

    mode = input("Mode (1-3, default 1): ").strip() or "1"

    nums = input("Enter numbers separated by spaces: ")
    try:
        numbers = [int(n) for n in nums.split()]
        if not numbers:
            print("No numbers given. Using default: 1 2 3 4 5 6 7 8 9")
            numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9]

        modes = {"1": draw_with_numbers, "2": draw_flower, "3": draw_polygons}
        modes.get(mode, draw_with_numbers)(numbers)

    except ValueError:
        print("Please enter valid whole numbers.")
