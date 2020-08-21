# == Schema Information
#
# Table name: transactions
#
#  id                   :bigint           not null, primary key
#  amount               :float            not null
#  date                 :datetime         not null
#  description          :string           not null
#  tags                 :string
#  transaction_category :string           not null
#  created_at           :datetime         not null
#  updated_at           :datetime         not null
#  account_id           :integer          not null
#
class Transaction < ApplicationRecord
  include PgSearch::Model
  pg_search_scope :search_for_transaction, against: [:description, :transaction_category, :date, :tags]
  validates_presence_of :amount, :date, :description, :transaction_category, :account_id
  validates :transaction_category, inclusion: { in: %w(Housing Transportation Food Utilities Healthcare Personal Recreation/Entertainment Shopping Miscellaneous Income Other)}
  belongs_to :account

  def update_account
    if (self.account.debit)
      self.account.balance += self.amount
    else
      self.account.balance -= self.amount
    end
  end

  def update_on_delete
    if (self.account.debit)
      self.account.balance -= self.amount 
    else
      self.account.balance += self.amount
    end
  end

  def update_on_change(old_amt)
    if (self.account.debit)
      self.account.balance += self.amount - old_amt
    else
      self.account.balance -= self.amount - old_amt
    end
  end
  

end
