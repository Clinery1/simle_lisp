import time


def fizzBuzz(max, i=1):
    three = (i % 3) == 0
    five = (i % 5) == 0
    if i <= max:
        if three and five:
            print("FizzBuzz")
        elif three:
            print("Fizz")
        elif five:
            print("Buzz")
        else:
            print(i)
        fizzBuzz(max, i + 1)


start = time.perf_counter()
fizzBuzz(30)
end = time.perf_counter()

seconds = end - start
millis = seconds * 1000.0
micros = millis * 1000.0
print(str(micros) + "µs")
