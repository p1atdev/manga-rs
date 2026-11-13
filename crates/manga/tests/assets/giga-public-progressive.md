# Progressive JPEG with EXIF fixture

Derived from `giga-tonari-yj-public-20261111.jpg` using `jpegtran -progressive -copy all` (lossless coefficient transcoding). A minimal empty EXIF IFD was inserted as APP1, without changing image data. Used to test strict validation and byte-exact preservation of a progressive JPEG containing ordinary metadata.
