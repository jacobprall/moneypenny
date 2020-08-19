
import React, {useState} from 'react'

export default function account_form({passedAccount, formType, errors, processForm, closeModal}) {
  

  const [account, setAccount] = useState(passedAccount)
  
  const update = (field) => {
    console.log(account)
    return e => (
      setAccount({...account, [field]: e.currentTarget.value,})
    )
  }

  const handleSubmit = (e) => {
    console.log(account)
    e.preventDefault();
    if (errors.length === 0) {
      processForm(account).then(closeModal());
    }
    
  }

  const handleToggle = (e) => {
    e.preventDefault();
    if (account.debit === false) {
      setAccount({...account, debit: true})
    } else {
      setAccount({ ...account, debit: false })
    }
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
  const institutions = [
    'Chase Bank',
    'J.P. Morgan',
    'Bank of America',
    'Merrill Lynch',
    'US Bank',
    'Citibank',
    'Wells Fargo',
    'Charles Schwab',
    'Fidelity',
    'Discover',
    'American Express',
    'Visa',
    'Other',
    'None'
  ]


  return (
    <div className="account-form-container">
      <form onSubmit={handleSubmit} className="account-form">
        <div onClick={closeModal} className="close-x">X</div>
        <div className="account-inputs">
          <label>Label:
            <input type="text" value={account.label} onChange={update('label')}/>
          </label>
          
          <label>Category:
            <select value={account.account_category} onChange={update('account_category')}>
              <option value="Cash">Cash</option>
              <option value="Credit Cards">Credit Cards</option>
              <option value="Loans">Loans</option>
              <option value="Investments">Investments</option>
              <option value="Property">Property</option>

            </select>
          </label>
          
          <label>Balance:
            <input type="number" min="0" step=".01" value={account.balance} onChange={update('balance')} />
          </label>
          
          <label>Institution:
            <select value={account.institution} onChange={update('institution')}>
              {institutions.map((inst, i) => (
                <option key={i} value={`${inst}`}>{inst}</option>
              ))}
            </select>
          </label>
          
          <label>Debit?
            <input type="checkbox" value={account.debit} onClick={handleToggle} />
          </label>
          <input type="submit" value={formType} />
          {renderErrors()}
        </div>
      </form>
    </div>
  )
}

