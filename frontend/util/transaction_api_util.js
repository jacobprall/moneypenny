export const fetchTransactions = () =>
  $.ajax({
    url: "/api/transactions",
  });

export const createTransaction = (transaction) =>
  $.ajax({
    url: "api/transactions",
    method: "POST",
    data: {
      transaction,
    },
  });

export const updateTransaction = (transaction) =>
  $.ajax({
    url: `api/transactions/${transaction.id}`,
    method: "PATCH",
    data: {
      transaction,
    },
  });

export const deleteTransaction = (transactionId) =>
  $.ajax({
    url: `api/transactions/${transactionId}`,
    method: "DELETE",
  });

export const searchTransaction = (searchParams) =>
  $.ajax({
    url: `api/transactions/search/${searchParams}`,
    method: "GET",
  });
