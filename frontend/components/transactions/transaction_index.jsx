import React, {useEffect} from 'react'
import TransactionLineItem from './transaction_line_item'
import TransactionFormContainer from './transaction_form/transaction_form_container'
import { useSelector, useDispatch, shallowEqual } from 'react-redux'
import { requestTransactions } from '../../actions/transaction_actions'
import { openModal } from '../../actions/modal_actions'

export default function transaction_index() {
  
  // request transactions
  const dispatch = useDispatch();
  const transactionsRequester = () => dispatch(requestTransactions())
  // useEffect(() => {
  //   transactionsRequester()
  // }, []);
  
  const transactions = useSelector((state) => Object.values(state.entities.transactions))
  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload))

  // dummy transaction creator
  const baseAccount = useSelector((state) => Object.values(state.entities.accounts)[0]);

  let accountId = {}
  if (baseAccount) {
    accountId = baseAccount.id
  }

  const newTransaction = {
    'amount': 0,
    'date': new Date(),
    'description': 'None',
    'transaction_category': "Miscellaneous",
    'tags': "",
    'account_id': `${accountId}`
  }

  let headers = ["amount", "date", "description", "transaction category"]

  // render functions 
  function renderTableHeader() {
    if (transactions.length) {
      return headers.map((k, index) => {
        if (k !== 'id' && k !== 'tags' && k !== 'account_id') {
          return <th key={index}>{k.toUpperCase()}</th>
        }
      })
    }
  }

  const renderTransactions = () => (
    transactions.reverse().map((transaction, i) => (
      <TransactionLineItem transaction={transaction} key={i} />
    ))
  );

  

  

  return (
    <div className="transactions-index-container">
      <div className="transactions">
        <div className="above-table">
          <button className="add-transaction" onClick={() => modalOpener('new', TransactionFormContainer, newTransaction)}>+ Add Transaction</button>
          <input type="text"/>
        </div>
        <table>
          <thead>
            <tr>
              {renderTableHeader()}
              <th className="delete-column"><img src={window.trashCan} className="trash-can" /></th>
            </tr>
            </thead>
          <tbody>
            {renderTransactions()}
          </tbody>
        </table>
      </div>
    </div>
  )
}
