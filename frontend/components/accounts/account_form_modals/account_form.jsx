
import React, {useState} from 'react'

export default function account_form({account, formType, errors, processForm, closeModal}) {

  const [acc, setAcc] = setState(account)
  const update = (field) => {
    return e => (
      acc[field] = e.currentTarget.value
    )
  }

  const handleSubmit = (e) => {
    e.preventDefault();
    const changeAccount = acc;
    processForm(changeAccount).then(closeModal());
  }

  const renderErrors = () => {
    return (
      <ul>
        {errors.map((error, i) => (
          <li key={i}>{error}</li>
        ))}
      </ul>
    )
  }


  return (
    <div className="account-form-container">
      <form onSubmit={handleSubmit} className="account-form">
        <div onClick={closeModal} className="close-x">X</div>
        <div className="account-inputs">
          <label>Label:
            <input type="text" value={account.label} onChange={update('label')}/>
          </label>
          <br/>
          <label>Category:
            <input type="text" value={account.account_category} onChange={update('account_category')} />
          </label>
          <br />
          <label>Balance:
            <input type="number" value={account.balance} onChange={update('balance')} />
          </label>
          <br/>
          <label>Institution:
            <input type="select" value={account.institution} onChange={update('institution')} />
          </label>
          <br/>
          <label>Debit?:
            <input type="radial" value={account.debit} onChange={update('debit')} />
          </label>
          <input type="submit" value={formType} />
          {renderErrors}
        </div>
      </form>
    </div>
  )
}

