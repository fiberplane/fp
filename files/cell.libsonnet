local generateId = std.native('generateId');

{
  text(content, readOnly=false, id=generateId()):: {
    type: 'text',
    id: id,
    content: content,
    readOnly: readOnly,
  },
  heading(content, readOnly=false, id=generateId()):: {
    type: 'text',
    id: id,
    content: content,
    readOnly: readOnly,
  },
}
