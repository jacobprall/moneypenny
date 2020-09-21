import * as TransactionAPIUtil from "../util/transaction_api_util";
import { receiveAccounts, requestAccounts } from "./account_actions";
export const RECEIVE_TRANSACTIONS = "RECEIVE_TRANSACTIONS";
export const RECEIVE_TRANSACTION = "RECEIVE_TRANSACTION";
export const REMOVE_TRANSACTION = "REMOVE_TRANSACTION";
export const RECEIVE_TRANSACTION_ERRORS = "RECEIVE_TRANSACTION_ERRORS";
export const CLEAR_TRANSACTION_ERRORS = "CLEAR_TRANSACTION_ERRORS";
export const RECEIVE_TRANSACTION_SEARCH = "RECEIVE_TRANSACTION_SEARCH";
export const CLEAR_TRANSACTION_SEARCH = "CLEAR_TRANSACTION_SEARCH";

export const receiveTransactions = (transactions) => ({
  type: RECEIVE_TRANSACTIONS,
  transactions,
});

export const postTransaction = (transaction) => ({
  type: RECEIVE_TRANSACTION,
  transaction,
});

export const patchTransaction = (transaction) => ({
  type: RECEIVE_TRANSACTION,
  transaction,
});

export const removeTransaction = (transactionId) => ({
  type: REMOVE_TRANSACTION,
  transactionId,
});

export const receiveTransactionErrors = (errors) => ({
  type: RECEIVE_TRANSACTION_ERRORS,
  errors,
});

export const clearTransactionErrors = () => ({
  type: CLEAR_TRANSACTION_ERRORS,
});

export const searchTransactions = (transactions) => ({
  type: RECEIVE_TRANSACTION_SEARCH,
  transactions,
});

export const clearTransactionSearch = (transactions) => ({
  type: CLEAR_TRANSACTION_SEARCH,
  transactions,
});

/////

export const requestTransactions = () => (dispatch) =>
  TransactionAPIUtil.fetchTransactions().then((transactions) =>
    dispatch(receiveTransactions(transactions))
  );

export const searchForTransactions = (searchParams) => (dispatch) =>
  TransactionAPIUtil.searchTransaction(searchParams).then((transactions) =>
    dispatch(searchTransactions(transactions))
  );

export const clearSearch = () => (dispatch) =>
  TransactionAPIUtil.fetchTransactions().then((transactions) =>
    dispatch(clearTransactionSearch(transactions))
  );

export const createTransaction = (transaction) => (dispatch) =>
  TransactionAPIUtil.createTransaction(transaction)
    .then(
      (transaction) => dispatch(postTransaction(transaction)),
      (errors) => dispatch(receiveTransactionErrors(errors.responseJSON))
    )
    .then(() => dispatch(requestAccounts()));

export const updateTransaction = (transaction) => (dispatch) =>
  TransactionAPIUtil.updateTransaction(transaction)
    .then(
      (transaction) => dispatch(patchTransaction(transaction)),
      (errors) => dispatch(receiveTransactionErrors(errors.responseJSON))
    )
    .then(() => dispatch(requestAccounts()));

export const deleteTransaction = (transactionId) => (dispatch) =>
  TransactionAPIUtil.deleteTransaction(transactionId)
    .then((transactionId) => dispatch(removeTransaction(transactionId)))
    .then(() => dispatch(requestAccounts()));
