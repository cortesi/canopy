local names = {}

for _, fixture in ipairs(fixtures()) do
    names[fixture.name] = true
end

canopy.assert(names.empty, "expected the empty fixture")
canopy.assert(names.with_items, "expected the with_items fixture")
canopy.assert(names.modal_open, "expected the modal_open fixture")
