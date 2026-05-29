import turtle



print("how many loops?")
range1 = int(input())
print("forward how much?")
forward = int(input())
print("curve? :3")
left = int(input())



for i in range(range1):
    turtle.forward(forward)
    turtle.left(left)


