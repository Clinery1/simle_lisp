def fakePrint(string):
    pass

def fizzBuzz(count):
    for i in range(0, count):
        three = (i % 3) == 0
        five = (i % 5) == 0

        if three and five:
            fakePrint("FizzBuzz")
        elif three:
            fakePrint("Fizz")
        elif five:
            fakePrint("Buzz")
        else:
            fakePrint(str(i))

def doNTimes(count, func):
    for i in range(0, count):
        func()

def doTheThing():
    fizzBuzz(50)

doNTimes(40_000, doTheThing)
