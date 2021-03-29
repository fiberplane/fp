{
  new(title):: {
    title: title,

    addCell(cell):: self {}
                    + { cells+: [
                      cell,
                    ] },
    addCells(cells):: self {}
                      + { cells+: cells },
  },

}
