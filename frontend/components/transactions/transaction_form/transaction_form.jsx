import React, { useState } from 'react'


export default function transaction_form({ props: {selectedData, transactionDeleter, processForm, modalCloser, transactionErrorsClearer} }) {
  
  const {errors, formType, passedTransaction, accounts} = selectedData
  const [transaction, setTransaction] = useState(passedTransaction)
  const accountsList = Object.values(accounts);
  

  const update = (field) => e => setTransaction({ ...transaction, [field]: e.currentTarget.value })
  
  const handleSubmit = (e) => {
    e.preventDefault();
    processForm(transaction).then(
      () => modalCloser()).then(
      () => transactionErrorsClearer())
  };

  const handleClose = (e) => {
    e.preventDefault();
    modalCloser();
    transactionErrorsClearer();
  };

  const renderErrors = () => (
    <ul className="modal-form-errors">
      {errors.map((error, i) => (
        <li className="modal-form-error" key={i}>{error}</li>
      ))}
    </ul>
  );

  
  const deleteOption = () => {
    if (formType === 'edit') {
      console.log(transaction.id)
      return (
        <span className='edit-delete' onClick={() => transactionDeleter(transaction.id)}>Delete Transaction</span>
      )
    }
  }

  const transaction_categories = "Housing Transportation Food Utilities Healthcare Personal Recreation Entertainment Shopping Miscellaneous Income Other".split(' ')

  

  return (
    <form onSubmit={handleSubmit} className="modal-form">
      <div onClick={handleClose} className="close-x">X</div>
      <div className="modal-inputs">
        <label>Description:
          <input type="text" value={transaction.description} onChange={update('description')} />
        </label>
        <label>Amount:
          <input type="number" step=".01" value={transaction.amount} onChange={update('amount')} />
        </label>
        <label>Transaction Category:
            <select value={transaction.transaction_category} onChange={update('transaction_category')}>
            {transaction_categories.map((tc, i) => (
              <option key={i} value={`${tc}`}>{tc}</option>
            ))}
          </select>
        </label>
        <label>Date:
          <input type="date" value={transaction.date} onChange={update('date')} />
        </label>
        <label>Tags:
          <input type="text" value={transaction.tags} onChange={update('tags')} />
        </label>
        <label>Account:
            <select value={transaction.account_id} onChange={update('account_id')}>
            {accountsList.map((account, i) => (
              <option key={i} value={account.id}>{account.label}</option>
            ))}
          </select>
        </label>
        <button className="modal-form-submit" value={formType}>{formType.toUpperCase()}</button>
        {deleteOption()}
        {renderErrors()}
      </div>
    </form>

  )
}


