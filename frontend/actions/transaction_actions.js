import * as TransactionAPIUtil from "../util/transaction_api_util";
import { receiveAccounts, requestAccounts } from "./account_actions";
export const RECEIVE_TRANSACTIONS = "RECEIVE_TRANSACTIONS";
export const RECEIVE_TRANSACTION = "RECEIVE_TRANSACTION";
export const REMOVE_TRANSACTION = "REMOVE_TRANSACTION";
export const RECEIVE_TRANSACTION_ERRORS = "RECEIVE_TRANSACTION_ERRORS";
export const CLEAR_TRANSACTION_ERRORS = "CLEAR_TRANSACTION_ERRORS";

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

/////

export const requestTransactions = () => (dispatch) =>
  TransactionAPIUtil.fetchTransactions().then((transactions) =>
    dispatch(receiveTransactions(transactions))
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
