# == Schema Information
#
# Table name: accounts
#
#  id               :bigint           not null, primary key
#  debit            :boolean          not null
#  account_category :string           not null
#  institution      :string           not null
#  label            :string           not null
#  balance          :float            not null
#  user_id          :integer          not null
#  created_at       :datetime         not null
#  updated_at       :datetime         not null
#
class Account < ApplicationRecord
  CATEGORIES = [
    'Cash',
    'Credit Cards',
    'Loans',
    'Investments',
    'Property'
  ]
  INSTITUTIONS = [
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
  validates_presence_of :debit, :account_category, :institution, :label, :balance, :user_id 
  validates :account_category, inclusion: { in: CATEGORIES }
  validates :institution, inclusion: { in: INSTITUTIONS }

  belongs_to :user
    
  
end
