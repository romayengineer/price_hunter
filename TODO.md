# TODO List

this todo list covers new features to implement, they are sorted in order of implementation the first ones need to be implemented first

- [ ] store all products in sqlite as well as in json, json is for easy reading from files for people, sqlite is for the app easy retrival.
- [ ] create a list of products with brand, product name and size in ML, this list is going to be used to match products found on sites agains the known list of products by brand name and size
- [ ] implement a fuzzy match package to fuzzy match the products found agains the list of known products, the match assigns to each found product an id from the list of known produts so products with slightly different names from different sites are match to be closest name on the list of known products
- [ ] create a function to return a 2d structure (product to provider price) so rows are products (with brand name and size) and columns are the provider (the web site where the price comes from) and the cell value is the price
- [ ] once the 2d structure is created implement al simple algorithm to compare all prices from all providers for each product and compute a score, then sort the providers from cheapest to most expensive, the cheapest provider is going to be the best to buy from
