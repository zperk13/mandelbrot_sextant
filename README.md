### I was wanting to do 3 things:
1. Make an interactive Mandelbrot set viewer
2. Test terminal rendering with an increased resolution over just blocks by using sextants
3. See if I can get a very detailed zoom using [BigRational](https://docs.rs/num/latest/num/type.BigRational.html)

### Results:
1. Success (zoom is a little weird, but not that bad)
2. Success. At the sacrifice of being able to use colors, horizontal resolution has doubled and vertical resolution has tripled
3. Fail. I did technically get it working, but it was taking over half an hour to render 1 frame, and I stopped it before I found out how long it would take because half an hour is already ridiculous
